use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};

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
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateRelation {
    pub issues: Vec<uuid::Uuid>,
    #[serde(default)]
    pub relation_type: Option<String>,
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

// ---- links ----

pub async fn list_links(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, issue_id)): axum::extract::Path<Scope>,
) -> Result<Json<Vec<Value>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::work_item::IssueLink>(
        "SELECT id, url FROM issue_links WHERE project_id = $1 AND issue_id = $2 AND deleted_at IS NULL ORDER BY created_at",
    )
    .bind(project_id)
    .bind(issue_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(rows.into_iter().map(|l| json!({"id": l.id, "url": l.url})).collect()))
}

pub async fn create_link(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, issue_id)): axum::extract::Path<Scope>,
    Json(body): Json<CreateLink>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    validate_link_create(&body).map_err(|e| anyhow::anyhow!(e))?;
    if !issue_exists(&st, project_id, issue_id).await? {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Issue not found"}))));
    }
    let row = sqlx::query_as::<_, common::models::work_item::IssueLink>(
        "INSERT INTO issue_links (id, title, url, metadata, issue_id, project_id, workspace_id, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, '{}', $3, $4, i.workspace_id, now(), now() FROM issues i WHERE i.id = $3 RETURNING id, url",
    )
    .bind(&body.title)
    .bind(&body.url)
    .bind(issue_id)
    .bind(project_id)
    .fetch_one(&st.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(json!({"id": row.id, "url": row.url}))))
}

pub async fn get_link(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, issue_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let row: Option<common::models::work_item::IssueLink> = sqlx::query_as(
        "SELECT id, url FROM issue_links WHERE id = $1 AND project_id = $2 AND issue_id = $3 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .bind(issue_id)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(l) => Ok((StatusCode::OK, Json(json!({"id": l.id, "url": l.url})))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Link not found"})))),
    }
}

pub async fn patch_link(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, issue_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<PatchLink>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if let Some(title) = &body.title {
        if title.chars().count() > 255 {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "title max length 255"}))));
        }
    }
    let n = sqlx::query(
        "UPDATE issue_links SET title = COALESCE($1, title), url = COALESCE($2, url), updated_at = now() WHERE id = $3 AND project_id = $4 AND issue_id = $5 AND deleted_at IS NULL",
    )
    .bind(&body.title)
    .bind(&body.url)
    .bind(pk)
    .bind(project_id)
    .bind(issue_id)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Link not found"}))));
    }
    Ok((StatusCode::OK, Json(json!({"id": pk}))))
}

pub async fn delete_link(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, issue_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    sqlx::query(
        "UPDATE issue_links SET deleted_at = now() WHERE id = $1 AND project_id = $2 AND issue_id = $3 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .bind(issue_id)
    .execute(&st.pool)
    .await?;
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
    let user: Option<uuid::Uuid> = Some(auth.0);
    if user.is_none() {
        return Ok(Json(json!({"results": []})));
    }
    let pattern = match &q.search {
        Some(s) if !s.trim().is_empty() => format!("%{}%", s.replace(['%', '_'], "")),
        _ => "%".to_string(),
    };
    let rows: Vec<(uuid::Uuid, String)> = sqlx::query_as(
        "SELECT i.id, i.name FROM issues i JOIN project_members pm ON pm.project_id = i.project_id JOIN workspaces w ON w.id = i.workspace_id WHERE w.slug = $1 AND pm.member_id = $2 AND pm.is_active = true AND i.name ILIKE $3 AND i.deleted_at IS NULL ORDER BY i.created_at DESC LIMIT 100",
    )
    .bind(&slug)
    .bind(user.unwrap())
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
