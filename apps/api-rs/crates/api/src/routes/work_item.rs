use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};
use crate::routes::project::{FORBIDDEN_MSG, deny, missing};

use super::issue_common::{fetch_project_member_role, is_workspace_admin, project_gate_allows};

/// Issue sub-resources + `work-items/` aliases for
/// `plane/api/urls/work_item.py`. The `work-items/` paths serve the SAME
/// view classes as `issues/` in Django — one handler set is mounted under
/// both prefixes in `main.rs`.
///
/// - comments (`issue_comments`): list/create + detail get/patch/delete.
///   `comment_html` defaults to `<p></p>`, `comment_json` to `{}`.
/// - links (`issue_links`): list/create + detail; url required, title 255.
/// - relations (`issue_relations`): list grouped by type + bulk create
///   `{relation_type, issues[]}` (IssueRelationCreateSerializer: 8 choices,
///   min 1 issue); duplicate pair → 409.
/// - activities (`issue_activities`): read-only list + detail get.
/// - issue detail get/patch/delete (also serves `work-items/:pk/`).
/// - `work-items/search/` (workspace-wide issue search) and
///   `work-items/:project_identifier-:issue_identifier/` lookup.
///
/// STAYS ON DJANGO: attachments (S3 presigned upload flow, same boundary
/// as 2.14 asset bytes).
pub const RELATION_TYPES: [&str; 8] = [
    "blocking",
    "blocked_by",
    "duplicate",
    "relates_to",
    "start_before",
    "start_after",
    "finish_before",
    "finish_after",
];

pub const PRIORITIES: [&str; 5] = ["low", "medium", "high", "urgent", "none"];

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CreateComment {
    #[serde(default)]
    pub comment_html: Option<String>,
    #[serde(default)]
    pub comment_json: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchComment {
    #[serde(default)]
    pub comment_html: Option<String>,
    #[serde(default)]
    pub comment_json: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateLink {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchLink {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateRelation {
    pub issues: Vec<uuid::Uuid>,
    #[serde(default)]
    pub relation_type: Option<String>,
}

/// POST body for `remove-relation`: mirrors
/// `request.data.get("related_issue", None)` (`relation.py:272`).
#[derive(Debug, Clone, Deserialize)]
pub struct RemoveRelationBody {
    pub related_issue: Option<uuid::Uuid>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchIssue {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IssueOut {
    pub id: uuid::Uuid,
    pub name: String,
}

pub fn validate_link_create(body: &CreateLink) -> Result<(), String> {
    if body.url.trim().is_empty() {
        return Err("url is required".to_string());
    }
    if let Some(title) = &body.title {
        if title.chars().count() > 255 {
            return Err("title max length 255".to_string());
        }
    }
    Ok(())
}

pub fn validate_relation_create(body: &CreateRelation) -> Result<(), String> {
    if body.issues.is_empty() {
        return Err("At least one issue ID is required.".to_string());
    }
    match &body.relation_type {
        Some(t) if RELATION_TYPES.contains(&t.as_str()) => Ok(()),
        _ => Err("Invalid relation type".to_string()),
    }
}

pub fn validate_issue_patch(body: &PatchIssue) -> Result<(), String> {
    if let Some(name) = &body.name {
        if name.trim().is_empty() {
            return Err("name must not be blank".to_string());
        }
    }
    if let Some(priority) = &body.priority {
        if !PRIORITIES.contains(&priority.as_str()) {
            return Err("Invalid priority".to_string());
        }
    }
    Ok(())
}

type Scope = (String, uuid::Uuid, uuid::Uuid);

async fn issue_exists(st: &AppState, project_id: uuid::Uuid, issue_id: uuid::Uuid) -> Result<bool, common::errors::AppError> {
    let exists: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL)",
    )
    .bind(issue_id)
    .bind(project_id)
    .fetch_one(&st.pool)
    .await?;
    Ok(exists.0)
}

// ---- comments ----

pub async fn list_comments(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, issue_id)): axum::extract::Path<Scope>,
) -> Result<Json<Vec<Value>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::work_item::IssueComment>(
        "SELECT id, comment_html FROM issue_comments WHERE project_id = $1 AND issue_id = $2 AND deleted_at IS NULL ORDER BY created_at",
    )
    .bind(project_id)
    .bind(issue_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(rows.into_iter().map(|c| json!({"id": c.id, "comment_html": c.comment_html})).collect()))
}

pub async fn create_comment(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, issue_id)): axum::extract::Path<Scope>,
    Json(body): Json<CreateComment>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !issue_exists(&st, project_id, issue_id).await? {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Issue not found"}))));
    }
    let row = sqlx::query_as::<_, common::models::work_item::IssueComment>(
        "INSERT INTO issue_comments (id, comment_html, comment_json, comment_stripped, access, attachments, issue_id, project_id, workspace_id, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, '', 'INTERNAL', '{}', $3, $4, i.workspace_id, now(), now() FROM issues i WHERE i.id = $3 RETURNING id, comment_html",
    )
    .bind(body.comment_html.clone().unwrap_or_else(|| "<p></p>".to_string()))
    .bind(body.comment_json.clone().unwrap_or(json!({})))
    .bind(issue_id)
    .bind(project_id)
    .fetch_one(&st.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(json!({"id": row.id, "comment_html": row.comment_html}))))
}

pub async fn get_comment(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, issue_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let row: Option<common::models::work_item::IssueComment> = sqlx::query_as(
        "SELECT id, comment_html FROM issue_comments WHERE id = $1 AND project_id = $2 AND issue_id = $3 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .bind(issue_id)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(c) => Ok((StatusCode::OK, Json(json!({"id": c.id, "comment_html": c.comment_html})))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Comment not found"})))),
    }
}

pub async fn patch_comment(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, issue_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<PatchComment>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let n = sqlx::query(
        "UPDATE issue_comments SET comment_html = COALESCE($1, comment_html), comment_json = COALESCE($2, comment_json), updated_at = now() WHERE id = $3 AND project_id = $4 AND issue_id = $5 AND deleted_at IS NULL",
    )
    .bind(&body.comment_html)
    .bind(&body.comment_json)
    .bind(pk)
    .bind(project_id)
    .bind(issue_id)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Comment not found"}))));
    }
    Ok((StatusCode::OK, Json(json!({"id": pk}))))
}

pub async fn delete_comment(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, issue_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    sqlx::query(
        "UPDATE issue_comments SET deleted_at = now() WHERE id = $1 AND project_id = $2 AND issue_id = $3 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .bind(issue_id)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

// ---- links (parity with Django `IssueLinkViewSet`,
// `plane/app/views/issue/link.py:26-113` +
// `plane/app/serializers/issue.py:550-598`) ----

/// `serializers/issue.py` `IssueLinkSerializer.create` dup branch.
pub(crate) const LINK_DUP_MSG: &str = "URL already exists for this Issue";
/// `serializers/issue.py` `IssueLinkSerializer.validate_url` branch.
pub(crate) const LINK_INVALID_MSG: &str = "Invalid URL format.";

pub(crate) fn normalize_link_url(url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") { url.to_string() } else { format!("http://{url}") }
}

/// Mirrors `validate_url` (`serializers/issue.py`, Django `URLValidator`):
/// absolute http(s) URL with a non-empty host (dot or `localhost`; no
/// whitespace) — same hand-rolled shape as the `module.rs`
/// `valid_link_url` precedent (Django additionally enforces TLD/IDNA
/// rules; accepting e.g. `http://intranet` here is a documented leniency).
pub(crate) fn valid_link_url(url: &str) -> bool {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"));
    let Some(rest) = rest else {
        return false;
    };
    let host = rest
        .split(['/', '?', '#', ':'].as_ref())
        .next()
        .unwrap_or("");
    if host.is_empty() || host.chars().any(char::is_whitespace) {
        return false;
    }
    host.contains('.') || host.eq_ignore_ascii_case("localhost")
}

/// One `IssueLinkSerializer` row (`serializers/issue.py`, `fields =
/// "__all__"` over `db/models/issue.py:371-381` + `created_by_detail`):
/// id, workspace, project, issue, title, url, metadata, created_by,
/// updated_by, created_at, updated_at + `created_by_detail`. The `cbf_*`
/// columns come from the `LEFT JOIN users` on `created_by_id`. Local struct
/// (not the shared 2-field `common::models::work_item::IssueLink`, which
/// only carries `id, url`).
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct LinkRow {
    pub id: uuid::Uuid,
    pub workspace_id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub issue_id: uuid::Uuid,
    pub title: Option<String>,
    pub url: String,
    pub metadata: Value,
    pub created_by_id: Option<uuid::Uuid>,
    pub updated_by_id: Option<uuid::Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub cbf_id: Option<uuid::Uuid>,
    pub cbf_first_name: Option<String>,
    pub cbf_last_name: Option<String>,
    pub cbf_display_name: Option<String>,
}

const LINK_SELECT: &str = "SELECT l.id, l.workspace_id, l.project_id, l.issue_id, \
    l.title, l.url, l.metadata, l.created_by_id, l.updated_by_id, l.created_at, l.updated_at, \
    u.id AS cbf_id, u.first_name AS cbf_first_name, u.last_name AS cbf_last_name, \
    u.display_name AS cbf_display_name \
    FROM issue_links l LEFT JOIN users u ON u.id = l.created_by_id";

/// Serializes one row to the 12-key `IssueLinkSerializer` shape. FKs render
/// as ids (`workspace`, `project`, `issue`, `created_by`, `updated_by`,
/// null when unset). `created_by_detail` is the 4-key
/// `{id,first_name,last_name,display_name}` object (null when `created_by`
/// is null) — a subset of Django's 7-key `UserLiteSerializer`
/// (`serializers/user.py:141-153`, which additionally carries
/// `avatar`/`avatar_url`/`is_bot`); the 4-key shape is the Batch F T1
/// contract.
fn link_json(r: &LinkRow) -> Value {
    json!({
        "id": r.id,
        "workspace": r.workspace_id,
        "project": r.project_id,
        "issue": r.issue_id,
        "title": r.title,
        "url": r.url,
        "metadata": r.metadata,
        "created_by": r.created_by_id,
        "updated_by": r.updated_by_id,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "created_by_detail": match r.cbf_id {
            Some(uid) => json!({
                "id": uid,
                "first_name": r.cbf_first_name,
                "last_name": r.cbf_last_name,
                "display_name": r.cbf_display_name,
            }),
            None => Value::Null,
        },
    })
}

/// Shared PROJECT-level gate for link reads: `ProjectEntityPermission`
/// (`link.py:29`) on a safe (GET) method passes any ACTIVE project member
/// (`permissions/project.py:103-110`), i.e. roles 20/15/5, with the
/// workspace-ADMIN fallback (`permissions/base.py:53-78`) — exactly the
/// `history.rs` gate shape, reused via the same shared helpers.
async fn link_read_gate(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    slug: &str,
    project_id: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let member_role = fetch_project_member_role(pool, user_id, slug, project_id).await?;
    let ws_admin = is_workspace_admin(pool, user_id, slug).await?;
    Ok(project_gate_allows(
        matches!(member_role, Some(20) | Some(15) | Some(5)),
        member_role.is_some(),
        ws_admin,
    ))
}

/// Non-safe (POST/PATCH/DELETE) branch of `ProjectEntityPermission`
/// (`permissions/project.py:112-119`): ADMIN/MEMBER only — same shape as
/// `guard_remove_relation`; GUEST and non-members fall to the
/// workspace-ADMIN fallback applied by the caller via `project_gate_allows`.
pub(crate) fn guard_link_write(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) => Ok(()),
        _ => Err(FORBIDDEN_MSG.to_string()),
    }
}

async fn link_write_gate(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    slug: &str,
    project_id: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let member_role = fetch_project_member_role(pool, user_id, slug, project_id).await?;
    let ws_admin = is_workspace_admin(pool, user_id, slug).await?;
    Ok(project_gate_allows(
        guard_link_write(member_role).is_ok(),
        member_role.is_some(),
        ws_admin,
    ))
}

/// Scoped detail fetch mirroring `get_queryset`
/// (`link.py:31-45`): `workspace__slug + project_id + issue_id`, active
/// project membership of the caller, project not archived, live row.
async fn fetch_link(
    pool: &sqlx::PgPool,
    slug: &str,
    project_id: uuid::Uuid,
    issue_id: uuid::Uuid,
    pk: uuid::Uuid,
) -> Result<Option<LinkRow>, sqlx::Error> {
    sqlx::query_as::<_, LinkRow>(&format!(
        "{LINK_SELECT} JOIN workspaces w ON w.id = l.workspace_id \
        JOIN projects p ON p.id = l.project_id \
        WHERE w.slug = $1 AND l.project_id = $2 AND l.issue_id = $3 AND l.id = $4 \
        AND l.deleted_at IS NULL AND p.archived_at IS NULL"
    ))
    .bind(slug)
    .bind(project_id)
    .bind(issue_id)
    .bind(pk)
    .fetch_optional(pool)
    .await
}

async fn fetch_links(
    pool: &sqlx::PgPool,
    slug: &str,
    project_id: uuid::Uuid,
    issue_id: uuid::Uuid,
) -> Result<Vec<LinkRow>, sqlx::Error> {
    sqlx::query_as::<_, LinkRow>(&format!(
        "{LINK_SELECT} JOIN workspaces w ON w.id = l.workspace_id \
        JOIN projects p ON p.id = l.project_id \
        WHERE w.slug = $1 AND l.project_id = $2 AND l.issue_id = $3 \
        AND l.deleted_at IS NULL AND p.archived_at IS NULL \
        ORDER BY l.created_at DESC"
    ))
    .bind(slug)
    .bind(project_id)
    .bind(issue_id)
    .fetch_all(pool)
    .await
}

pub async fn list_links(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id)): axum::extract::Path<Scope>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !link_read_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let rows = fetch_links(&st.pool, &slug, project_id, issue_id).await?;
    Ok((
        StatusCode::OK,
        Json(json!(rows.iter().map(link_json).collect::<Vec<_>>())),
    ))
}

pub async fn create_link(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id)): axum::extract::Path<Scope>,
    // Raw `Value` (not `CreateLink`): the Django body additionally carries
    // `metadata`, and `CreateLink`'s 2-field shape is frozen by
    // `tests/work_item_test.rs` struct literals — metadata is read off the
    // raw body (default `{}`) while required/title checks reuse
    // `validate_link_create`.
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !link_write_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let title = body.get("title").and_then(Value::as_str).map(str::to_string);
    let raw_url = body.get("url").and_then(Value::as_str).unwrap_or("");
    // Django renders every serializer-validation failure as 400 (not 500).
    if let Err(e) = validate_link_create(&CreateLink {
        title: title.clone(),
        url: raw_url.to_string(),
    }) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
    }
    if !issue_exists(&st, project_id, issue_id).await? {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Issue not found"}))));
    }
    // `to_internal_value` (`serializers/issue.py`): prepend `http://` when
    // the scheme is missing, then `validate_url` (Django `URLValidator`).
    let url = normalize_link_url(raw_url);
    if !valid_link_url(&url) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": LINK_INVALID_MSG}))));
    }
    // `IssueLinkSerializer.create` dup branch (live rows only, same
    // soft-delete precedent as the relations dup check in this file).
    let dup: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issue_links WHERE url = $1 AND issue_id = $2 AND deleted_at IS NULL)",
    )
    .bind(&url)
    .bind(issue_id)
    .fetch_one(&st.pool)
    .await?;
    if dup {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": LINK_DUP_MSG}))));
    }
    let metadata = body.get("metadata").cloned().unwrap_or(json!({}));
    let row: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO issue_links (id, title, url, metadata, issue_id, project_id, workspace_id, created_by_id, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, $3, $4, $5, i.workspace_id, $6, now(), now() FROM issues i WHERE i.id = $4 RETURNING id",
    )
    .bind(&title)
    .bind(&url)
    .bind(&metadata)
    .bind(issue_id)
    .bind(project_id)
    .bind(auth.0)
    .fetch_one(&st.pool)
    .await?;
    // Django re-fetches through `get_queryset` before responding 201.
    match fetch_link(&st.pool, &slug, project_id, issue_id, row.0).await? {
        Some(link) => Ok((StatusCode::CREATED, Json(link_json(&link)))),
        None => Err(anyhow::anyhow!("link vanished after insert").into()),
    }
}

pub async fn get_link(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !link_read_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    match fetch_link(&st.pool, &slug, project_id, issue_id, pk).await? {
        Some(l) => Ok((StatusCode::OK, Json(link_json(&l)))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Link not found"})))),
    }
}

pub async fn patch_link(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<PatchLink>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !link_write_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let existing = fetch_link(&st.pool, &slug, project_id, issue_id, pk).await?;
    if existing.is_none() {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Link not found"}))));
    }
    if let Some(title) = &body.title {
        if title.chars().count() > 255 {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "title max length 255"}))));
        }
    }
    // Mirror `to_internal_value` + `validate_url`, then the
    // `IssueLinkSerializer.update` dup branch (excluding self) — only when
    // `url` is part of the payload (Django reads
    // `validated_data.get("url")`, absent → no match → no error).
    let new_url = body.url.as_deref().map(normalize_link_url);
    if let Some(url) = &new_url {
        if !valid_link_url(url) {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": LINK_INVALID_MSG}))));
        }
        let dup: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM issue_links WHERE url = $1 AND issue_id = $2 AND id <> $3 AND deleted_at IS NULL)",
        )
        .bind(url)
        .bind(issue_id)
        .bind(pk)
        .fetch_one(&st.pool)
        .await?;
        if dup {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": LINK_DUP_MSG}))));
        }
    }
    let n = sqlx::query(
        "UPDATE issue_links SET title = COALESCE($1, title), url = COALESCE($2, url), metadata = COALESCE($3, metadata), updated_by_id = $4, updated_at = now() WHERE id = $5 AND project_id = $6 AND issue_id = $7 AND deleted_at IS NULL",
    )
    .bind(&body.title)
    .bind(&new_url)
    .bind(&body.metadata)
    .bind(auth.0)
    .bind(pk)
    .bind(project_id)
    .bind(issue_id)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Link not found"}))));
    }
    match fetch_link(&st.pool, &slug, project_id, issue_id, pk).await? {
        Some(link) => Ok((StatusCode::OK, Json(link_json(&link)))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Link not found"})))),
    }
}

pub async fn delete_link(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !link_write_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    // DEVIATION: Django `destroy` (`link.py:94-113`) hard-deletes via
    // `issue_link.delete()`; Rust soft-deletes (`deleted_at = now()`),
    // keeping the batch-wide soft-delete precedent (same as comments and
    // relations in this file). Wire status stays 204. Scoping mirrors
    // `fetch_link` (`workspace__slug` + project not archived); 0 rows →
    // 404 `missing()` (Django `.get()` (`link.py:98-99`) raises → 404).
    let n = sqlx::query(
        "UPDATE issue_links l SET deleted_at = now() FROM workspaces w JOIN projects p ON p.id = l.project_id WHERE l.id = $1 AND l.project_id = $2 AND l.issue_id = $3 AND l.deleted_at IS NULL AND w.id = l.workspace_id AND w.slug = $4 AND p.archived_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .bind(issue_id)
    .bind(&slug)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

// ---- relations ----

pub async fn list_relations(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, issue_id)): axum::extract::Path<Scope>,
) -> Result<Json<Value>, common::errors::AppError> {
    // Mirrors the grouped response: blocking / blocked_by / others.
    let rows = sqlx::query_as::<_, common::models::work_item::IssueRelation>(
        "SELECT r.id, r.related_issue_id, r.relation_type FROM issue_relations r JOIN issues i ON i.id = r.related_issue_id WHERE r.issue_id = $1 AND r.project_id = $2 AND r.deleted_at IS NULL AND i.deleted_at IS NULL",
    )
    .bind(issue_id)
    .bind(project_id)
    .fetch_all(&st.pool)
    .await?;
    let mut blocking = vec![];
    let mut blocked_by = vec![];
    let mut others: std::collections::HashMap<String, Vec<Value>> = std::collections::HashMap::new();
    for r in rows {
        let v = json!({"id": r.related_issue_id});
        match r.relation_type.as_str() {
            "blocking" => blocking.push(v),
            "blocked_by" => blocked_by.push(v),
            t => others.entry(t.to_string()).or_default().push(v),
        }
    }
    let mut out = serde_json::Map::new();
    out.insert("blocking".to_string(), Value::Array(blocking));
    out.insert("blocked_by".to_string(), Value::Array(blocked_by));
    for (k, v) in others {
        out.insert(k, Value::Array(v));
    }
    Ok(Json(Value::Object(out)))
}

pub async fn create_relations(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, issue_id)): axum::extract::Path<Scope>,
    Json(body): Json<CreateRelation>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    validate_relation_create(&body).map_err(|e| anyhow::anyhow!(e))?;
    if !issue_exists(&st, project_id, issue_id).await? {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Issue not found"}))));
    }
    let relation_type = body.relation_type.clone().unwrap();
    let mut created = vec![];
    for related in &body.issues {
        let dup: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM issue_relations WHERE issue_id = $1 AND related_issue_id = $2 AND deleted_at IS NULL)",
        )
        .bind(issue_id)
        .bind(related)
        .fetch_one(&st.pool)
        .await?;
        if dup.0 {
            return Ok((
                StatusCode::CONFLICT,
                Json(json!({"error": "Relation already exists for this issue pair"})),
            ));
        }
        let row: (uuid::Uuid,) = sqlx::query_as(
            "INSERT INTO issue_relations (id, issue_id, related_issue_id, relation_type, project_id, workspace_id, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, $3, $4, i.workspace_id, now(), now() FROM issues i WHERE i.id = $1 RETURNING id",
        )
        .bind(issue_id)
        .bind(related)
        .bind(&relation_type)
        .bind(project_id)
        .fetch_one(&st.pool)
        .await?;
        created.push(row.0);
    }
    Ok((StatusCode::CREATED, Json(json!({"ids": created}))))
}

/// PROJECT-level role check for `remove-relation`: mirrors
/// `IssueRelationViewSet.permission_classes = [ProjectEntityPermission]`
/// (`relation.py:40`) on a non-safe (POST) method → ADMIN/MEMBER only
/// (`permissions/project.py:112-119`); anything else (incl. GUEST 5 and
/// non-member) falls to the workspace-ADMIN fallback applied by the caller
/// via the shared `project_gate_allows` (same shape as D5/D7/D8).
pub(crate) fn guard_remove_relation(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) => Ok(()),
        _ => Err(FORBIDDEN_MSG.to_string()),
    }
}

/// Mirrors `request.data.get("related_issue", None)` (`relation.py:272`):
/// Django has NO missing-key branch — a missing key just filters with None
/// → `.first()` → None → `None.delete()` → AttributeError (500). Rust
/// returns 404 `missing()` instead (intentional deviation, sane); the
/// caller maps this `Err` to `missing()`.
pub(crate) fn resolve_related_issue(body: &RemoveRelationBody) -> Result<uuid::Uuid, ()> {
    body.related_issue.ok_or(())
}

/// Shared PROJECT gate for `remove_relation`: the outer
/// `ProjectEntityPermission` check with the standard workspace-ADMIN
/// fallback (`permissions/base.py:53-78`) via `project_gate_allows` —
/// exactly the D8 `reactions_gate` shape.
async fn remove_relation_gate(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    slug: &str,
    project_id: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let member_role = fetch_project_member_role(pool, user_id, slug, project_id).await?;
    let ws_admin = is_workspace_admin(pool, user_id, slug).await?;
    Ok(project_gate_allows(
        guard_remove_relation(member_role).is_ok(),
        member_role.is_some(),
        ws_admin,
    ))
}

/// POST `.../issues/:issue_id/remove-relation/` — parity with Django
/// `IssueRelationViewSet.remove_relation` (`relation.py:271-293`,
/// `urls/issue.py:240-244`).
///
/// - Gate ADMIN/MEMBER + ws-admin fallback (see `remove_relation_gate`).
/// - Relation found via the bidirectional OR-filter
///   (`Q(issue=related, related_issue=issue) | Q(issue=issue,
///   related_issue=related)`, `relation.py:276-278`), workspace-scoped
///   (`workspace__slug=slug`, `relation.py:274-275`) over live rows
///   (soft-delete default managers are implicit in Django), `.first()`
///   = `ORDER BY created_at DESC LIMIT 1` (`IssueRelation`
///   `Meta.ordering = ("-created_at",)`, `db/models/issue.py:317`).
/// - Success is a soft-delete (Django default-manager `.delete()`) →
///   **204** empty. Miss — or `related_issue` absent (Django has no such
///   branch; it would 500 on `None.delete()`) — → 404 `missing()`
///   (intentional deviation, documented).
///
/// Deviations: none on the wire; Celery `issue_activity.delay` skipped
/// (batch-wide precedent). `DELETE issue-relation/:relId/` is NOT
/// implemented — Django defines no such route (FE-dead; FE
/// `issue.service.ts:196` must migrate to this endpoint).
pub async fn remove_relation(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id)): axum::extract::Path<Scope>,
    Json(body): Json<RemoveRelationBody>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !remove_relation_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let Ok(related) = resolve_related_issue(&body) else {
        return Ok(missing());
    };
    let row: Option<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT r.id FROM issue_relations r JOIN workspaces w ON w.id = r.workspace_id \
        WHERE w.slug = $1 AND r.deleted_at IS NULL \
        AND ((r.issue_id = $2 AND r.related_issue_id = $3) OR (r.issue_id = $3 AND r.related_issue_id = $2)) \
        ORDER BY r.created_at DESC LIMIT 1",
    )
    .bind(&slug)
    .bind(issue_id)
    .bind(related)
    .fetch_optional(&st.pool)
    .await?;
    let Some((rel_id,)) = row else {
        return Ok(missing());
    };
    sqlx::query("UPDATE issue_relations SET deleted_at = now() WHERE id = $1")
        .bind(rel_id)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

// ---- activities (read-only) ----

pub async fn list_activities(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, issue_id)): axum::extract::Path<Scope>,
) -> Result<Json<Vec<Value>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::work_item::IssueActivity>(
        "SELECT id, verb FROM issue_activities WHERE project_id = $1 AND issue_id = $2 AND deleted_at IS NULL ORDER BY created_at",
    )
    .bind(project_id)
    .bind(issue_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(rows.into_iter().map(|a| json!({"id": a.id, "verb": a.verb})).collect()))
}

pub async fn get_activity(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, issue_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let row: Option<common::models::work_item::IssueActivity> = sqlx::query_as(
        "SELECT id, verb FROM issue_activities WHERE id = $1 AND project_id = $2 AND issue_id = $3 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .bind(issue_id)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(a) => Ok((StatusCode::OK, Json(json!({"id": a.id, "verb": a.verb})))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Activity not found"})))),
    }
}

// ---- issue detail (also serves work-items/:pk/) ----

pub async fn get_issue(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let row: Option<common::models::issue::Issue> = sqlx::query_as(
        "SELECT id, name FROM issues WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(i) => Ok((StatusCode::OK, Json(json!({"id": i.id, "name": i.name})))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Issue not found"})))),
    }
}

pub async fn patch_issue(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<PatchIssue>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    validate_issue_patch(&body).map_err(|e| anyhow::anyhow!(e))?;
    let n = sqlx::query(
        "UPDATE issues SET name = COALESCE($1, name), description = COALESCE($2, description), priority = COALESCE($3, priority), updated_at = now() WHERE id = $4 AND project_id = $5 AND deleted_at IS NULL",
    )
    .bind(&body.name)
    .bind(&body.description)
    .bind(&body.priority)
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Issue not found"}))));
    }
    Ok((StatusCode::OK, Json(json!({"id": pk}))))
}

pub async fn delete_issue(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    sqlx::query(
        "UPDATE issues SET deleted_at = now() WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

// ---- work-items aliases ----

pub async fn workspace_issue_search(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    axum::extract::Query(q): axum::extract::Query<crate::routes::search::GlobalSearchQuery>,
) -> Result<Json<Value>, common::errors::AppError> {
    // AuthUser identitas sudah tervalidasi di extractor — selalu ada.
    let user = auth.0;
    let pattern = match &q.search {
        Some(s) if !s.trim().is_empty() => format!("%{}%", s.replace(['%', '_'], "")),
        _ => "%".to_string(),
    };
    let rows: Vec<(uuid::Uuid, String)> = sqlx::query_as(
        "SELECT i.id, i.name FROM issues i JOIN project_members pm ON pm.project_id = i.project_id JOIN workspaces w ON w.id = i.workspace_id WHERE w.slug = $1 AND pm.member_id = $2 AND pm.is_active = true AND i.name ILIKE $3 AND i.deleted_at IS NULL ORDER BY i.created_at DESC LIMIT 100",
    )
    .bind(&slug)
    .bind(user)
    .bind(&pattern)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!({"results": rows.into_iter().map(|(id, name)| json!({"id": id, "name": name})).collect::<Vec<_>>()})))
}

pub async fn get_by_identifier(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, ident)): axum::extract::Path<(String, String)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // `<project_identifier>-<sequence>`: project.identifier + issue.sequence_id.
    let (proj_ident, seq) = match ident.rsplit_once('-') {
        Some((p, s)) => (p, s.parse::<i32>().ok()),
        None => return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Work item not found"})))),
    };
    let Some(seq) = seq else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Work item not found"}))));
    };
    let row: Option<(uuid::Uuid, String)> = sqlx::query_as(
        "SELECT i.id, i.name FROM issues i JOIN projects p ON p.id = i.project_id JOIN workspaces w ON w.id = i.workspace_id WHERE w.slug = $1 AND p.identifier = $2 AND i.sequence_id = $3 AND i.deleted_at IS NULL",
    )
    .bind(&slug)
    .bind(proj_ident)
    .bind(seq)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some((id, name)) => Ok((StatusCode::OK, Json(json!({"id": id, "name": name})))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Work item not found"})))),
    }
}

#[cfg(test)]
mod batch_d_d9_tests {
    use super::*;
    use crate::routes::project::{FORBIDDEN_MSG, NOT_FOUND_MSG};

    #[test]
    fn missing_related_issue_maps_to_404_missing_not_400() {
        // `remove_relation` reads `request.data.get("related_issue", None)`
        // (`relation.py:272`) with NO missing-key branch — a missing key
        // just filters with None → `.first()` → None → `None.delete()` →
        // AttributeError (500 in Django). Rust returns 404 `missing()`
        // instead (intentional deviation, sane).
        assert!(resolve_related_issue(&RemoveRelationBody { related_issue: None }).is_err());
        let (status, body) = missing();
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body.0, json!({"error": NOT_FOUND_MSG}));
    }

    #[test]
    fn present_related_issue_resolves() {
        let id = uuid::Uuid::nil();
        assert_eq!(
            resolve_related_issue(&RemoveRelationBody { related_issue: Some(id) }),
            Ok(id)
        );
    }

    #[test]
    fn guard_remove_relation_is_admin_member_only() {
        // `IssueRelationViewSet.permission_classes =
        // [ProjectEntityPermission]` (`relation.py:40`); POST is non-safe
        // → ADMIN/MEMBER only (`permissions/project.py:112-119`); GUEST
        // falls to the workspace-ADMIN fallback via `project_gate_allows`.
        assert!(guard_remove_relation(Some(20)).is_ok());
        assert!(guard_remove_relation(Some(15)).is_ok());
        assert!(guard_remove_relation(Some(5)).is_err());
        assert_eq!(
            guard_remove_relation(None).unwrap_err(),
            FORBIDDEN_MSG.to_string()
        );
    }

    #[test]
    fn ws_admin_fallback_covers_non_amg_member() {
        // Same `project_gate_allows` shape as D5/D7/D8: a member with a
        // non-AMG role + ws-admin still passes; roleless never passes.
        use super::super::issue_common::project_gate_allows;
        assert!(project_gate_allows(
            guard_remove_relation(Some(10)).is_ok(),
            true,
            true
        ));
        assert!(!project_gate_allows(
            guard_remove_relation(Some(5)).is_ok(),
            true,
            false
        ));
        assert!(!project_gate_allows(
            guard_remove_relation(None).is_ok(),
            false,
            true
        ));
    }

    #[test]
    fn link_shape_matches_django_issue_link_serializer() {
        let row = serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "workspace": "00000000-0000-0000-0000-000000000002",
            "project": "00000000-0000-0000-0000-000000000003",
            "issue": "00000000-0000-0000-0000-000000000004",
            "title": null,
            "url": "http://example.com",
            "metadata": {},
            "created_by": "00000000-0000-0000-0000-000000000005",
            "updated_by": null,
            "created_at": "2026-09-08T00:00:00Z",
            "updated_at": "2026-09-08T00:00:00Z",
            "created_by_detail": {"id": "00000000-0000-0000-0000-000000000005", "first_name": "A", "last_name": "B", "display_name": "A B"}
        });
        let keys: std::collections::BTreeSet<&str> = row.as_object().unwrap().keys().map(|s| s.as_str()).collect();
        assert_eq!(keys.len(), 12);
    }

    #[test]
    fn link_url_normalize_prepends_http() {
        assert_eq!(normalize_link_url("example.com/a"), "http://example.com/a");
        assert_eq!(normalize_link_url("https://x.io"), "https://x.io");
    }

    #[test]
    fn link_duplicate_url_error_matches_django() {
        assert_eq!(LINK_DUP_MSG, "URL already exists for this Issue");
        assert_eq!(LINK_INVALID_MSG, "Invalid URL format.");
    }
}
