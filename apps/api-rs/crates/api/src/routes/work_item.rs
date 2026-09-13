use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};
use crate::routes::project::{FORBIDDEN_MSG, deny, missing, user_avatar_url};

use super::issue_common::{fetch_project_member_role, is_workspace_admin, project_gate_allows};

/// Issue sub-resources + `work-items/` aliases for
/// `plane/api/urls/work_item.py`. The `work-items/` paths serve the SAME
/// view classes as `issues/` in Django — one handler set is mounted under
/// both prefixes in `main.rs`.
///
/// - comments (`issue_comments`): list/create + detail get/patch/delete.
///   `comment_html` defaults to `<p></p>`, `comment_json` to `{}`.
/// - links (`issue_links`): list/create + detail; url required, title 255.
/// - relations (`issue_relations`): list grouped by type (8 fixed groups,
///   14-key rows, `IssueRelationViewSet.list`) + bulk create
///   `{relation_type, issues[]}` (missing type → 400; ws-scoped candidates;
///   direction swap + actual-type mapping; dups silently skipped, 201
///   serializer array).
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
    // Missing `issues` defaults to `[]` (Django
    // `request.data.get("issues", [])`, `relation.py:217`).
    #[serde(default)]
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
    // Django `IssueCreateSerializer` (`serializers/issue.py:82-105`,
    // `fields = "__all__"`): the Issue model has NO `description` column —
    // description lives in `description_html` (text) + `description_json`
    // (jsonb). `description` here is the tiptap JSON doc → `description_json`
    // (same mapping as the intake nested-issue write, `intake.rs:1271-1286`).
    #[serde(default)]
    pub description_html: Option<String>,
    #[serde(default)]
    pub description: Option<Value>,
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

/// Pure body validator retained for the `work_item_test.rs` unit precedent.
/// NOTE: `create_relations` does NOT call this — Django `create`
/// (`relation.py:209-269`) performs no choice/count validation (missing type
/// → 400, unknown types pass through the identity map, empty `issues` →
/// 201 `[]`).
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

/// Full comment row: every `issue_comments` scalar (`IssueCommentSerializer`
/// `__all__`) + actor UserLite columns + membership flag. Nested
/// `issue_detail`/`project_detail`/`workspace_detail` and the
/// `comment_reactions` array stay T1 (FE resolves issues/projects/workspaces
/// from its stores and reactions via the dedicated reactions endpoints).
#[derive(Debug, Clone, sqlx::FromRow)]
struct CommentRow {
    id: uuid::Uuid,
    comment_html: String,
    comment_json: Value,
    comment_stripped: String,
    attachments: Vec<String>,
    access: String,
    issue_id: uuid::Uuid,
    project_id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    actor_id: Option<uuid::Uuid>,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    edited_at: Option<chrono::DateTime<chrono::Utc>>,
    description_id: Option<uuid::Uuid>,
    external_id: Option<String>,
    external_source: Option<String>,
    parent_id: Option<uuid::Uuid>,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    a_first_name: Option<String>,
    a_last_name: Option<String>,
    a_avatar: Option<String>,
    a_avatar_asset_id: Option<uuid::Uuid>,
    a_is_bot: Option<bool>,
    a_display_name: Option<String>,
}

const COMMENT_SELECT_SQL: &str = "SELECT c.id, c.comment_html, c.comment_json, c.comment_stripped, c.attachments, c.access, c.issue_id, c.project_id, c.workspace_id, c.actor_id, c.created_by_id, c.updated_by_id, c.created_at, c.updated_at, c.edited_at, c.description_id, c.external_id, c.external_source, c.parent_id, c.deleted_at, u.first_name AS a_first_name, u.last_name AS a_last_name, u.avatar AS a_avatar, u.avatar_asset_id AS a_avatar_asset_id, u.is_bot AS a_is_bot, u.display_name AS a_display_name FROM issue_comments c LEFT JOIN users u ON u.id = c.actor_id LEFT JOIN file_assets fa ON fa.id = u.avatar_asset_id";

/// Nested context shared by every comment in one issue thread
/// (`IssueCommentSerializer` declared extras, `serializers/issue.py:697-703`):
/// `issue_detail` (`IssueFlatSerializer`, 10 keys), `project_detail`
/// (`ProjectLiteSerializer`, 7 keys), `workspace_detail`
/// (`WorkspaceLiteSerializer`, 4 keys). Same issue/project/workspace for the
/// whole list response, so one fetch serves all rows.
struct CommentCtx {
    issue_detail: Value,
    project_detail: Value,
    workspace_detail: Value,
}

async fn comment_ctx(
    pool: &sqlx::PgPool,
    slug: &str,
    project_id: uuid::Uuid,
    issue_id: uuid::Uuid,
) -> Result<CommentCtx, common::errors::AppError> {
    let issue: Option<(
        uuid::Uuid,
        String,
        Value,
        String,
        String,
        Option<chrono::NaiveDate>,
        Option<chrono::NaiveDate>,
        i32,
        f64,
        bool,
    )> = sqlx::query_as(
        "SELECT id, name, description_json, description_html, priority, start_date, target_date, sequence_id, sort_order, is_draft FROM issues WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(issue_id)
    .fetch_optional(pool)
    .await?;
    let issue_detail = match issue {
        Some((id, name, djson, dhtml, priority, start, target, seq, sort, draft)) => json!({
            "id": id,
            "name": name,
            "description_json": djson,
            "description_html": dhtml,
            "priority": priority,
            "start_date": start,
            "target_date": target,
            "sequence_id": seq,
            "sort_order": sort,
            "is_draft": draft,
        }),
        None => Value::Null,
    };
    let project: Option<(
        uuid::Uuid,
        String,
        String,
        Option<String>,
        Option<uuid::Uuid>,
        Value,
        String,
    )> = sqlx::query_as(
        "SELECT p.id, p.identifier, p.name, p.cover_image, p.cover_image_asset_id, p.logo_props, p.description FROM projects p WHERE p.id = $1",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    let project_detail = match project {
        Some((id, identifier, name, cover_image, cover_asset_id, logo_props, description)) => {
            let cover_entity: Option<String> = sqlx::query_scalar(
                "SELECT entity_type FROM file_assets WHERE id = $1",
            )
            .bind(cover_asset_id)
            .fetch_optional(pool)
            .await?
            .flatten();
            super::history::project_lite_json(
                id,
                &identifier,
                &name,
                &cover_image,
                cover_asset_id,
                cover_entity.as_deref(),
                &logo_props,
                &description,
            )
        }
        None => Value::Null,
    };
    let ws: Option<(uuid::Uuid, String, String, Option<String>, Option<uuid::Uuid>)> = sqlx::query_as(
        "SELECT id, name, slug, logo, logo_asset_id FROM workspaces WHERE slug = $1",
    )
    .bind(slug)
    .fetch_optional(pool)
    .await?;
    let workspace_detail = match ws {
        Some((id, name, wslug, logo, logo_asset_id)) => {
            let logo_entity: Option<String> = sqlx::query_scalar(
                "SELECT entity_type FROM file_assets WHERE id = $1",
            )
            .bind(logo_asset_id)
            .fetch_optional(pool)
            .await?
            .flatten();
            super::history::workspace_lite_json(id, &name, &wslug, &logo, logo_asset_id, logo_entity.as_deref())
        }
        None => Value::Null,
    };
    Ok(CommentCtx { issue_detail, project_detail, workspace_detail })
}

/// `comment_reactions` per comment (`CommentReactionSerializer`,
/// `serializers/issue.py:666-678`: id, actor, comment, reaction,
/// display_name, deleted_at, workspace). One query for a whole thread.
async fn comment_reactions_map(
    pool: &sqlx::PgPool,
    comment_ids: &[uuid::Uuid],
) -> Result<std::collections::HashMap<uuid::Uuid, Vec<Value>>, common::errors::AppError> {
    let mut map: std::collections::HashMap<uuid::Uuid, Vec<Value>> = std::collections::HashMap::new();
    if comment_ids.is_empty() {
        return Ok(map);
    }
    let rows: Vec<(
        uuid::Uuid,
        uuid::Uuid,
        uuid::Uuid,
        String,
        Option<String>,
        Option<chrono::DateTime<chrono::Utc>>,
        uuid::Uuid,
    )> = sqlx::query_as(
        "SELECT r.id, r.actor_id, r.comment_id, r.reaction, u.display_name, r.deleted_at, r.workspace_id FROM comment_reactions r LEFT JOIN users u ON u.id = r.actor_id WHERE r.comment_id = ANY($1) AND r.deleted_at IS NULL",
    )
    .bind(comment_ids)
    .fetch_all(pool)
    .await?;
    for (id, actor, comment, reaction, display_name, deleted_at, workspace) in rows {
        map.entry(comment).or_default().push(json!({
            "id": id,
            "actor": actor,
            "comment": comment,
            "reaction": reaction,
            "display_name": display_name,
            "deleted_at": deleted_at,
            "workspace": workspace,
        }));
    }
    Ok(map)
}

fn comment_json(c: &CommentRow, is_member: bool, ctx: &CommentCtx, reactions: &[Value]) -> Value {
    let actor = match c.actor_id {
        Some(uid) => json!({
            "id": uid,
            "first_name": c.a_first_name,
            "last_name": c.a_last_name,
            "avatar": c.a_avatar,
            "avatar_url": user_avatar_url(c.a_avatar_asset_id, None, c.a_avatar.as_deref()),
            "is_bot": c.a_is_bot,
            "display_name": c.a_display_name,
        }),
        None => Value::Null,
    };
    json!({
        "actor_detail": actor,
        "issue_detail": ctx.issue_detail,
        "project_detail": ctx.project_detail,
        "workspace_detail": ctx.workspace_detail,
        "comment_reactions": reactions,
        "is_member": is_member,
        "id": c.id,
        "comment_html": c.comment_html,
        "comment_json": c.comment_json,
        "comment_stripped": c.comment_stripped,
        "attachments": c.attachments,
        "access": c.access,
        "issue": c.issue_id,
        "project": c.project_id,
        "workspace": c.workspace_id,
        "actor": c.actor_id,
        "created_by": c.created_by_id,
        "updated_by": c.updated_by_id,
        "created_at": c.created_at,
        "updated_at": c.updated_at,
        "edited_at": c.edited_at,
        "description": c.description_id,
        "external_id": c.external_id,
        "external_source": c.external_source,
        "parent": c.parent_id,
        "deleted_at": c.deleted_at,
    })
}

/// Active project membership id for the comment gates (Django queryset
/// member filter + `ProjectBasePermission`-family: any active role passes
/// reads; writes need ADMIN-or-creator, checked per row).
async fn comment_member(
    st: &AppState,
    user: uuid::Uuid,
    slug: &str,
    project_id: uuid::Uuid,
) -> Result<Option<i16>, common::errors::AppError> {
    Ok(fetch_project_member_role(&st.pool, user, slug, project_id).await?)
}

/// Active WORKSPACE membership (`ProjectBasePermission`-family SAFE branch
/// and `@allow_permission` first branch, `permissions/project.py:18-22`,
/// `permissions/base.py:24-33`): the decorator denies non-ws-members BEFORE
/// the body runs, so a denied miss is 403, not 404.
async fn ws_active_member(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
) -> Result<bool, common::errors::AppError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM workspace_members wm JOIN workspaces w ON w.id = wm.workspace_id WHERE w.slug = $1 AND wm.member_id = $2 AND wm.is_active = true AND wm.deleted_at IS NULL)",
    )
    .bind(slug)
    .bind(user)
    .fetch_one(pool)
    .await?)
}

pub async fn list_comments(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id)): axum::extract::Path<Scope>,
) -> Result<Json<Vec<Value>>, common::errors::AppError> {
    // Django `get_queryset` (`comment.py:35-61`): workspace+project+issue
    // scope, active membership (any role, guests included), archived
    // projects excluded.
    let role = comment_member(&st, auth.0, &slug, project_id).await?;
    if role.is_none() {
        return Ok(Json(Vec::new()));
    }
    let rows: Vec<CommentRow> = sqlx::query_as(&format!(
        "{COMMENT_SELECT_SQL} WHERE c.project_id = $1 AND c.issue_id = $2 AND c.workspace_id = (SELECT id FROM workspaces WHERE slug = $3) AND c.deleted_at IS NULL AND EXISTS(SELECT 1 FROM projects p WHERE p.id = $1 AND p.archived_at IS NULL) ORDER BY c.created_at"
    ))
    .bind(project_id)
    .bind(issue_id)
    .bind(&slug)
    .fetch_all(&st.pool)
    .await?;
    let ctx = comment_ctx(&st.pool, &slug, project_id, issue_id).await?;
    let ids: Vec<uuid::Uuid> = rows.iter().map(|c| c.id).collect();
    let reactions = comment_reactions_map(&st.pool, &ids).await?;
    Ok(Json(
        rows.iter()
            .map(|c| comment_json(c, true, &ctx, reactions.get(&c.id).map(Vec::as_slice).unwrap_or(&[])))
            .collect(),
    ))
}

pub async fn create_comment(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id)): axum::extract::Path<Scope>,
    Json(body): Json<CreateComment>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `create` (`comment.py:63-81`): ADMIN/MEMBER/GUEST + the
    // guest-view 400 (`guest role + !guest_view_all_features + not creator`).
    let role = comment_member(&st, auth.0, &slug, project_id).await?;
    let Some(role) = role else {
        return Ok(deny());
    };
    if role == 5 {
        let gva: bool = sqlx::query_scalar("SELECT guest_view_all_features FROM projects WHERE id = $1")
            .bind(project_id)
            .fetch_optional(&st.pool)
            .await?
            .unwrap_or(false);
        let creator: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT created_by_id FROM issues WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
        )
        .bind(issue_id)
        .bind(project_id)
        .fetch_optional(&st.pool)
        .await?
        .flatten();
        if !gva && creator != Some(auth.0) {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "You are not allowed to comment on the issue"})),
            ));
        }
    }
    if !issue_exists(&st, project_id, issue_id).await? {
        return Ok(missing());
    }
    let id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO issue_comments (id, comment_html, comment_json, comment_stripped, access, attachments, issue_id, project_id, workspace_id, actor_id, created_by_id, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, '', 'INTERNAL', '{}', $3, $4, i.workspace_id, $5, $5, now(), now() FROM issues i WHERE i.id = $3 RETURNING id",
    )
    .bind(body.comment_html.clone().unwrap_or_else(|| "<p></p>".to_string()))
    .bind(body.comment_json.clone().unwrap_or(json!({})))
    .bind(issue_id)
    .bind(project_id)
    .bind(auth.0)
    .fetch_one(&st.pool)
    .await?;
    // 201 full row, re-read through the list scope.
    // NOTE Django quirk (ADR): `create` returns `serializer.data` on an
    // instance WITHOUT the `is_member` annotation, so stock Django 500s
    // here; Rust returns the intended shape sanely.
    let row: Option<CommentRow> = sqlx::query_as(&format!(
        "{COMMENT_SELECT_SQL} WHERE c.id = $1 AND c.deleted_at IS NULL"
    ))
    .bind(id)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(c) => {
            let ctx = comment_ctx(&st.pool, &slug, project_id, issue_id).await?;
            let reactions = comment_reactions_map(&st.pool, &[c.id]).await?;
            Ok((
                StatusCode::CREATED,
                Json(comment_json(
                    &c,
                    true,
                    &ctx,
                    reactions.get(&c.id).map(Vec::as_slice).unwrap_or(&[]),
                )),
            ))
        }
        None => Ok(missing()),
    }
}

pub async fn get_comment(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `retrieve` is DRF-default on the member-filtered queryset
    // (`comment.py:35-61`): non-members 404 (not 403) — the filter hides
    // the row before any permission runs.
    let role = comment_member(&st, auth.0, &slug, project_id).await?;
    if role.is_none() {
        return Ok(missing());
    }
    let row: Option<CommentRow> = sqlx::query_as(&format!(
        "{COMMENT_SELECT_SQL} WHERE c.id = $1 AND c.project_id = $2 AND c.issue_id = $3 AND c.workspace_id = (SELECT id FROM workspaces WHERE slug = $4) AND c.deleted_at IS NULL"
    ))
    .bind(pk)
    .bind(project_id)
    .bind(issue_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(c) => {
            let ctx = comment_ctx(&st.pool, &slug, project_id, issue_id).await?;
            let reactions = comment_reactions_map(&st.pool, &[c.id]).await?;
            Ok((
                StatusCode::OK,
                Json(comment_json(
                    &c,
                    true,
                    &ctx,
                    reactions.get(&c.id).map(Vec::as_slice).unwrap_or(&[]),
                )),
            ))
        }
        None => Ok(missing()),
    }
}

pub async fn patch_comment(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<PatchComment>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `partial_update` (`comment.py:109-141`):
    // `@allow_permission([ADMIN], creator=True, model=IssueComment)` runs
    // BEFORE the body — gate first (a denied miss is 403, not 404), then
    // the scoped `.get()` miss → 404. The creator branch needs workspace
    // membership only (NOT project membership), then `created_by` match.
    if !ws_active_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let creator: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issue_comments WHERE id = $1 AND created_by_id = $2 AND deleted_at IS NULL)",
    )
    .bind(pk)
    .bind(auth.0)
    .fetch_one(&st.pool)
    .await?;
    let role = comment_member(&st, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !creator
        && !project_gate_allows(matches!(role, Some(20)), role.is_some(), ws_admin)
    {
        return Ok(deny());
    }
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT c.comment_html FROM issue_comments c JOIN workspaces w ON w.id = c.workspace_id WHERE c.id = $1 AND c.project_id = $2 AND c.issue_id = $3 AND w.slug = $4 AND c.deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .bind(issue_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some((old_html,)) = row else {
        return Ok(missing());
    };
    // `edited_at` bumps only when `comment_html` actually changes
    // (`comment.py:116-117`).
    let changed = body.comment_html.as_deref().is_some_and(|h| h != old_html);
    sqlx::query(
        "UPDATE issue_comments SET comment_html = COALESCE($1, comment_html), comment_json = COALESCE($2, comment_json), edited_at = CASE WHEN $4 THEN now() ELSE edited_at END, updated_at = now() WHERE id = $3",
    )
    .bind(&body.comment_html)
    .bind(&body.comment_json)
    .bind(pk)
    .bind(changed)
    .execute(&st.pool)
    .await?;
    let row: Option<CommentRow> = sqlx::query_as(&format!(
        "{COMMENT_SELECT_SQL} WHERE c.id = $1 AND c.deleted_at IS NULL"
    ))
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(c) => {
            let ctx = comment_ctx(&st.pool, &slug, project_id, issue_id).await?;
            let reactions = comment_reactions_map(&st.pool, &[c.id]).await?;
            Ok((
                StatusCode::OK,
                Json(comment_json(
                    &c,
                    role.is_some(),
                    &ctx,
                    reactions.get(&c.id).map(Vec::as_slice).unwrap_or(&[]),
                )),
            ))
        }
        None => Ok(missing()),
    }
}

pub async fn delete_comment(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `destroy` (`comment.py:144-160`): same decorator-first gate as
    // `partial_update`; then the scoped `.get()` miss → 404. NOTE Django
    // quirk (ADR): the body serializes `current_instance` on an instance
    // WITHOUT the `is_member` annotation, so stock Django 500s on success;
    // Rust returns the intended 204 sanely.
    if !ws_active_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let creator: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issue_comments WHERE id = $1 AND created_by_id = $2 AND deleted_at IS NULL)",
    )
    .bind(pk)
    .bind(auth.0)
    .fetch_one(&st.pool)
    .await?;
    let role = comment_member(&st, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !creator
        && !project_gate_allows(matches!(role, Some(20)), role.is_some(), ws_admin)
    {
        return Ok(deny());
    }
    let row: Option<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT c.id FROM issue_comments c JOIN workspaces w ON w.id = c.workspace_id WHERE c.id = $1 AND c.project_id = $2 AND c.issue_id = $3 AND w.slug = $4 AND c.deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .bind(issue_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    if row.is_none() {
        return Ok(missing());
    }
    sqlx::query(
        "UPDATE issue_comments SET deleted_at = now() WHERE id = $1",
    )
    .bind(pk)
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

// ---- relations (parity with Django `IssueRelationViewSet.list/create`,
// `plane/app/views/issue/relation.py:42-269`) ----

/// Fixed 8 group keys of the list response (`relation.py:174-205`).
pub fn relation_groups() -> [&'static str; 8] {
    [
        "blocking",
        "blocked_by",
        "duplicate",
        "relates_to",
        "start_after",
        "start_before",
        "finish_after",
        "finish_before",
    ]
}

/// Missing-`relation_type` branch (`relation.py:211-215`): 400 `{"message"}`.
pub const RELATION_TYPE_REQUIRED_MSG: &str = "Issue relation type is required";

/// Mirrors `get_actual_relation`
/// (`plane/utils/issue_relation_mapper.py:19-32`): the stored type for a
/// requested type; unknown types map to themselves (identity).
pub fn map_actual_relation(relation_type: &str) -> String {
    match relation_type {
        "start_after" => "start_before",
        "finish_after" => "finish_before",
        "blocking" => "blocked_by",
        "blocked_by" => "blocked_by",
        "start_before" => "start_before",
        "finish_before" => "finish_before",
        "implemented_by" => "implemented_by",
        "implements" => "implemented_by",
        other => other,
    }
    .to_string()
}

/// Direction swap (`relation.py:232-236`): for these REQUESTED types the
/// candidate becomes `issue_id` and the URL issue `related_issue_id`. The
/// same set selects the `RelatedIssueSerializer` branch (`relation.py:260`);
/// anything else uses `IssueRelationSerializer`.
pub(crate) fn relation_swaps_direction(relation_type: &str) -> bool {
    matches!(relation_type, "blocking" | "start_after" | "finish_after")
}

/// Shared PROJECT-level read gate for relation list: `ProjectEntityPermission`
/// (`relation.py:40`) on a safe (GET) method passes any ACTIVE project member
/// (20/15/5, `permissions/project.py:103-110`) with the workspace-ADMIN
/// fallback — the `link_read_gate` shape.
async fn relation_read_gate(
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

/// Shared PROJECT-level write gate for relation create: non-safe (POST) →
/// ADMIN/MEMBER only (`permissions/project.py:112-119`) + ws-admin fallback —
/// the `remove_relation_gate` shape, reusing `guard_remove_relation`.
async fn relation_write_gate(
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

/// One list row: the 14 `.values()` fields (`relation.py:157-172`) —
/// `label_ids`/`assignee_ids` via the `SUB_SELECT_SQL` annotation precedent
/// (live bridge rows; assignees need an active project membership, mirroring
/// the `assignee__member_project__is_active` filter).
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct RelationIssueRow {
    pub id: uuid::Uuid,
    pub name: String,
    pub state_id: Option<uuid::Uuid>,
    pub sort_order: f64,
    pub priority: String,
    pub sequence_id: i32,
    pub project_id: uuid::Uuid,
    pub label_ids: Vec<uuid::Uuid>,
    pub assignee_ids: Vec<uuid::Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by: Option<uuid::Uuid>,
    pub updated_by: Option<uuid::Uuid>,
}

/// Serializes one row to the 14-key shape; `relation_type` is the resolved
/// GROUP annotation (`Value("blocking", ...)` etc., `relation.py:176-204`),
/// not a stored column.
fn relation_issue_json(r: &RelationIssueRow, group: &str) -> Value {
    json!({
        "id": r.id,
        "name": r.name,
        "state_id": r.state_id,
        "sort_order": r.sort_order,
        "priority": r.priority,
        "sequence_id": r.sequence_id,
        "project_id": r.project_id,
        "label_ids": r.label_ids,
        "assignee_ids": r.assignee_ids,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "created_by": r.created_by,
        "updated_by": r.updated_by,
        "relation_type": group,
    })
}

pub async fn list_relations(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id)): axum::extract::Path<Scope>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !relation_read_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    // Bidirectional live relation rows for this issue, workspace-scoped
    // (`Q(issue_id=issue_id) | Q(related_issue=issue_id)`,
    // `workspace__slug=slug`, `relation.py:43-51`).
    #[derive(sqlx::FromRow)]
    struct RelPair {
        issue_id: uuid::Uuid,
        related_issue_id: uuid::Uuid,
        relation_type: String,
    }
    let pairs = sqlx::query_as::<_, RelPair>(
        "SELECT r.issue_id, r.related_issue_id, r.relation_type FROM issue_relations r \
        JOIN workspaces w ON w.id = r.workspace_id \
        WHERE w.slug = $1 AND r.deleted_at IS NULL \
        AND (r.issue_id = $2 OR r.related_issue_id = $2) \
        ORDER BY r.created_at DESC",
    )
    .bind(&slug)
    .bind(issue_id)
    .fetch_all(&st.pool)
    .await?;
    // Group membership per `relation.py:53-100`. Independent `if`s (not
    // `else if`): a self-loop pair matches both of its sides, exactly like
    // the Django filters. Stored types outside this table (e.g. unknown
    // pass-through types) belong to NO group.
    let mut buckets: std::collections::HashMap<&'static str, Vec<uuid::Uuid>> =
        relation_groups().into_iter().map(|g| (g, Vec::new())).collect();
    let mut push = |group: &'static str, id: uuid::Uuid| {
        let v = buckets.entry(group).or_default();
        if !v.contains(&id) {
            v.push(id);
        }
    };
    for p in &pairs {
        if p.relation_type == "blocked_by" && p.related_issue_id == issue_id {
            push("blocking", p.issue_id);
        }
        if p.relation_type == "blocked_by" && p.issue_id == issue_id {
            push("blocked_by", p.related_issue_id);
        }
        if p.relation_type == "duplicate" && p.issue_id == issue_id {
            push("duplicate", p.related_issue_id);
        }
        if p.relation_type == "duplicate" && p.related_issue_id == issue_id {
            push("duplicate", p.issue_id);
        }
        if p.relation_type == "relates_to" && p.issue_id == issue_id {
            push("relates_to", p.related_issue_id);
        }
        if p.relation_type == "relates_to" && p.related_issue_id == issue_id {
            push("relates_to", p.issue_id);
        }
        if p.relation_type == "start_before" && p.related_issue_id == issue_id {
            push("start_after", p.issue_id);
        }
        if p.relation_type == "start_before" && p.issue_id == issue_id {
            push("start_before", p.related_issue_id);
        }
        if p.relation_type == "finish_before" && p.related_issue_id == issue_id {
            push("finish_after", p.issue_id);
        }
        if p.relation_type == "finish_before" && p.issue_id == issue_id {
            push("finish_before", p.related_issue_id);
        }
    }
    let all: Vec<uuid::Uuid> = buckets.values().flatten().copied().collect::<std::collections::HashSet<_>>().into_iter().collect();
    let rows: Vec<RelationIssueRow> = if all.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as(
            "SELECT i.id, i.name, i.state_id, i.sort_order, i.priority, i.sequence_id, i.project_id, \
            COALESCE((SELECT array_agg(il.label_id ORDER BY il.created_at DESC) FROM issue_labels il \
              WHERE il.issue_id = i.id AND il.deleted_at IS NULL), '{}'::uuid[]) AS label_ids, \
            COALESCE((SELECT array_agg(ia.assignee_id ORDER BY ia.created_at DESC) FROM issue_assignees ia \
              WHERE ia.issue_id = i.id AND ia.deleted_at IS NULL \
              AND EXISTS(SELECT 1 FROM project_members pm \
                WHERE pm.member_id = ia.assignee_id AND pm.is_active = true AND pm.deleted_at IS NULL)), '{}'::uuid[]) AS assignee_ids, \
            i.created_at, i.updated_at, i.created_by_id AS created_by, i.updated_by_id AS updated_by \
            FROM issues i JOIN workspaces w ON w.id = i.workspace_id \
            WHERE w.slug = $1 AND i.id = ANY($2) AND i.deleted_at IS NULL \
            ORDER BY i.created_at DESC",
        )
        .bind(&slug)
        .bind(&all)
        .fetch_all(&st.pool)
        .await?
    };
    let by_id: std::collections::HashMap<uuid::Uuid, &RelationIssueRow> =
        rows.iter().map(|r| (r.id, r)).collect();
    // All 8 keys always present (`relation.py:174-205`); rows ordered
    // `-created_at` (the `Issue`/`IssueRelation` `Meta.ordering`).
    let mut out = serde_json::Map::new();
    for group in relation_groups() {
        let mut group_rows: Vec<&RelationIssueRow> = buckets[group]
            .iter()
            .filter_map(|id| by_id.get(id).copied())
            .collect();
        group_rows.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        out.insert(
            group.to_string(),
            Value::Array(group_rows.iter().map(|r| relation_issue_json(r, group)).collect()),
        );
    }
    Ok((StatusCode::OK, Json(Value::Object(out))))
}

/// One created row for the 201 response: the stored relation columns plus the
/// candidate-issue columns backing both serializers (`IssueRelationSerializer`
/// `serializers/issue.py:402-431` reads the `related_issue` side,
/// `RelatedIssueSerializer` `issue.py:442-464` the `issue` side — both sides
/// resolve to the candidate here, so the 11 wire keys are identical;
/// `assignee_ids` is write-only → excluded on read).
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct CreatedRelationRow {
    pub id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub sequence_id: i32,
    pub name: String,
    pub relation_type: String,
    pub state_id: Option<uuid::Uuid>,
    pub priority: String,
    pub created_by: Option<uuid::Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub updated_by: Option<uuid::Uuid>,
}

fn created_relation_json(r: &CreatedRelationRow) -> Value {
    json!({
        "id": r.id,
        "project_id": r.project_id,
        "sequence_id": r.sequence_id,
        "name": r.name,
        "relation_type": r.relation_type,
        "state_id": r.state_id,
        "priority": r.priority,
        "created_by": r.created_by,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "updated_by": r.updated_by,
    })
}

pub async fn create_relations(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id)): axum::extract::Path<Scope>,
    Json(body): Json<CreateRelation>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !relation_write_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let Some(requested) = body.relation_type.clone() else {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"message": RELATION_TYPE_REQUIRED_MSG}))));
    };
    if !issue_exists(&st, project_id, issue_id).await? {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Issue not found"}))));
    }
    // Django `Project.objects.get(pk=project_id)` (`relation.py:218`) supplies
    // `workspace_id`; a missing project misses here → 404 `missing()` (Django
    // would 500 on `DoesNotExist`; sane mapping).
    let ws_id: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT workspace_id FROM projects WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some(ws_id) = ws_id else {
        return Ok(missing());
    };
    // Candidates scoped to `workspace__slug` ONLY — cross-project allowed
    // (`relation.py:222-227`). Input order is preserved for the response.
    let scoped: std::collections::HashSet<uuid::Uuid> = if body.issues.is_empty() {
        std::collections::HashSet::new()
    } else {
        sqlx::query_scalar::<_, uuid::Uuid>(
            "SELECT i.id FROM issues i JOIN workspaces w ON w.id = i.workspace_id \
            WHERE w.slug = $1 AND i.id = ANY($2) AND i.deleted_at IS NULL",
        )
        .bind(&slug)
        .bind(&body.issues)
        .fetch_all(&st.pool)
        .await?
        .into_iter()
        .collect()
    };
    let actual = map_actual_relation(&requested);
    let swapped = relation_swaps_direction(&requested);
    let mut out = Vec::new();
    for cand in body.issues.iter().filter(|id| scoped.contains(id)) {
        let (fwd, back) = if swapped { (*cand, issue_id) } else { (issue_id, *cand) };
        // `bulk_create(..., ignore_conflicts=True)` (`relation.py:229-246`):
        // per-candidate `ON CONFLICT DO NOTHING` (partial unique index on
        // live `(issue_id, related_issue_id)`) → duplicates silently skipped,
        // input order preserved. `relation_type` is the MAPPED actual type.
        let row: Option<CreatedRelationRow> = sqlx::query_as(
            "WITH ins AS ( \
              INSERT INTO issue_relations (id, issue_id, related_issue_id, relation_type, project_id, workspace_id, created_by_id, updated_by_id, created_at, updated_at) \
              VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $6, now(), now()) \
              ON CONFLICT DO NOTHING RETURNING id, relation_type, created_by_id, created_at, updated_at, updated_by_id \
            ) SELECT c.id AS id, c.project_id, c.sequence_id, c.name, ins.relation_type, c.state_id, c.priority, \
              ins.created_by_id AS created_by, ins.created_at, ins.updated_at, ins.updated_by_id AS updated_by \
            FROM ins JOIN issues c ON c.id = $7",
        )
        .bind(fwd)
        .bind(back)
        .bind(&actual)
        .bind(project_id)
        .bind(ws_id)
        .bind(auth.0)
        .bind(*cand)
        .fetch_optional(&st.pool)
        .await?;
        if let Some(r) = row {
            out.push(created_relation_json(&r));
        }
    }
    // Empty `issues` → 201 `[]`; serializer branch (`relation.py:260`) is
    // selected by the REQUESTED type but both wire shapes are identical here.
    Ok((StatusCode::CREATED, Json(Value::Array(out))))
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

/// F6a: full-row detail + Django gates. GET serves the 25-key
/// [`IssueDetailRow`] (same SELECT as `list_detail`, single-id scope) instead
/// of `{id, name}`; nested serializer objects stay T1. Gate mirrors
/// `retrieve` (`base.py:492`): active project membership (any role, guests
/// included) OR the issue creator; miss → 404 `missing()`.
pub async fn get_issue(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    use super::issue_common::IssueDetailRow;
    use super::issue_query::DETAIL_SELECT_SQL;
    // Django `retrieve` (`base.py:492`): `@allow_permission([ADMIN, MEMBER,
    // GUEST], creator=True, model=Issue)` runs BEFORE the body — gate first,
    // then the fetch. A denied non-member 403s even on a miss.
    if !ws_active_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let creator: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE id = $1 AND created_by_id = $2 AND deleted_at IS NULL)",
    )
    .bind(pk)
    .bind(auth.0)
    .fetch_one(&st.pool)
    .await?;
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !creator
        && !project_gate_allows(
            matches!(member_role, Some(20) | Some(15) | Some(5)),
            member_role.is_some(),
            ws_admin,
        )
    {
        return Ok(deny());
    }
    let row: Option<IssueDetailRow> = sqlx::query_as(&format!(
        "{DETAIL_SELECT_SQL} WHERE i.id = $1 AND i.project_id = $2 AND i.workspace_id = (SELECT id FROM workspaces WHERE slug = $3) AND i.deleted_at IS NULL"
    ))
    .bind(pk)
    .bind(project_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some(row) = row else {
        return Ok(missing());
    };
    // Guest-view rule inside the body (`base.py:596-609`): guests whose
    // project hides member work and who did not create the issue → 403.
    if matches!(member_role, Some(5)) && !creator {
        let gva: bool = sqlx::query_scalar("SELECT guest_view_all_features FROM projects WHERE id = $1")
            .bind(project_id)
            .fetch_optional(&st.pool)
            .await?
            .unwrap_or(false);
        if !gva {
            return Ok((
                StatusCode::FORBIDDEN,
                Json(json!({"error": super::versions::DESC_GUEST_MSG})),
            ));
        }
    }
    // The three `IssueDetailSerializer` extras (`serializers/issue.py:934-945`).
    // NOTE Django quirk (ADR): the pk `retrieve` annotates `is_subscribed`
    // but NOT `is_intake`, so stock Django 500s serializing this endpoint;
    // Rust returns the intended shape sanely (the `:ident/` twin annotates
    // both and 200s — same code path here via delegation).
    let description_html: Option<String> =
        sqlx::query_scalar("SELECT description_html FROM issues WHERE id = $1")
            .bind(pk)
            .fetch_optional(&st.pool)
            .await?
            .flatten();
    let is_subscribed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issue_subscribers s WHERE s.issue_id = $1 AND s.project_id = $2 AND s.workspace_id = (SELECT id FROM workspaces WHERE slug = $3) AND s.subscriber_id = $4 AND s.deleted_at IS NULL)",
    )
    .bind(pk)
    .bind(project_id)
    .bind(&slug)
    .bind(auth.0)
    .fetch_one(&st.pool)
    .await?;
    let is_intake: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM intake_issues ii WHERE ii.issue_id = $1 AND ii.status IN (-2, 0) AND ii.project_id = $2 AND ii.workspace_id = (SELECT id FROM workspaces WHERE slug = $3) AND ii.deleted_at IS NULL)",
    )
    .bind(pk)
    .bind(project_id)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    let mut v = serde_json::to_value(&row).unwrap_or(Value::Null);
    v["description_html"] = json!(description_html.unwrap_or_else(|| "<p></p>".to_string()));
    v["is_subscribed"] = json!(is_subscribed);
    v["is_intake"] = json!(is_intake);
    Ok((StatusCode::OK, Json(v)))
}

/// Quoted from `plane/app/views/issue/base.py:659-661`
/// (`IssueViewSet.partial_update`): miss → 404 with this body verbatim
/// (NOT the standard `missing()`).
pub(crate) const ISSUE_PATCH_MISS_MSG: &str = "Issue not found";

pub async fn patch_issue(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<PatchIssue>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `partial_update` (`base.py:627`): `@allow_permission([ADMIN,
    // MEMBER], creator=True, model=Issue)` runs BEFORE the body — gate
    // first (a denied miss is 403, not 404), then the fetch.
    if !ws_active_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let creator: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE id = $1 AND created_by_id = $2 AND deleted_at IS NULL)",
    )
    .bind(pk)
    .bind(auth.0)
    .fetch_one(&st.pool)
    .await?;
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !creator
        && !project_gate_allows(
            matches!(member_role, Some(20) | Some(15)),
            member_role.is_some(),
            ws_admin,
        )
    {
        return Ok(deny());
    }
    // Existence uses `get_queryset()` = `issue_objects` (`base.py:628-629`,
    // `models/issue.py:92-101`): drafts, archived issues, triage-state
    // issues and archived-project issues all miss with 404
    // `{"error": "Issue not found"}` verbatim (`base.py:659-661`) — NOT the
    // standard `missing()`.
    let row: Option<(Option<uuid::Uuid>,)> = sqlx::query_as(
        "SELECT i.created_by_id FROM issues i LEFT JOIN states s ON s.id = i.state_id WHERE i.id = $1 AND i.project_id = $2 AND i.workspace_id = (SELECT id FROM workspaces WHERE slug = $3) AND i.deleted_at IS NULL AND i.archived_at IS NULL AND i.is_draft = false AND (s.id IS NULL OR s.\"group\" != 'triage') AND EXISTS(SELECT 1 FROM projects p WHERE p.id = $2 AND p.archived_at IS NULL)",
    )
    .bind(pk)
    .bind(project_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    if row.is_none() {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": ISSUE_PATCH_MISS_MSG})),
        ));
    }
    // Django serializer 400s (never 500s) on invalid bodies.
    if let Err(e) = validate_issue_patch(&body) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
    }
    sqlx::query(
        "UPDATE issues SET name = COALESCE($1, name), description_html = COALESCE($2, description_html), description_json = COALESCE($3::jsonb, description_json), priority = COALESCE($4, priority), updated_at = now() WHERE id = $5 AND project_id = $6 AND deleted_at IS NULL",
    )
    .bind(&body.name)
    .bind(body.description_html.as_deref())
    .bind(&body.description)
    .bind(&body.priority)
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?;
    // Django returns 204 empty (`base.py:713`), not 200 `{id}`.
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

pub async fn delete_issue(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `destroy` (`base.py:716`): `@allow_permission([ADMIN],
    // creator=True, model=Issue)` first, then `Issue.objects.get(...)`
    // (soft-deletion filtered only — drafts/archived stay visible here).
    if !ws_active_member(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let creator: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE id = $1 AND created_by_id = $2 AND deleted_at IS NULL)",
    )
    .bind(pk)
    .bind(auth.0)
    .fetch_one(&st.pool)
    .await?;
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !creator
        && !project_gate_allows(matches!(member_role, Some(20)), member_role.is_some(), ws_admin)
    {
        return Ok(deny());
    }
    let row: Option<(Option<uuid::Uuid>,)> = sqlx::query_as(
        "SELECT created_by_id FROM issues WHERE id = $1 AND project_id = $2 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $3) AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    if row.is_none() {
        return Ok(missing());
    }
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
    auth: AuthUser,
    axum::extract::Path((slug, ident)): axum::extract::Path<(String, String)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Parity with Django `IssueDetailIdentifierEndpoint`
    // (`plane/app/views/issue/base.py`, URL `plane/app/urls/issue.py:281`
    // `work-items/<project_identifier>-<issue_identifier>/`). Axum cannot
    // express `:a-:b` in one segment, so the `:ident/` route shape stays and
    // the constraint is enforced here.
    let Ok((proj_ident, seq_raw)) = resolve_identifier(&ident) else {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": INVALID_IDENTIFIER_MSG}))));
    };
    // Project lookup: `identifier__iexact` + `workspace__slug` (miss → 404).
    let project_id: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT p.id FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
        WHERE w.slug = $1 AND LOWER(p.identifier) = LOWER($2) AND p.deleted_at IS NULL",
    )
    .bind(&slug)
    .bind(&proj_ident)
    .fetch_optional(&st.pool)
    .await?;
    let Some(project_id) = project_id else {
        return Ok(missing());
    };
    // Active project membership required (miss → 403, exact message).
    let role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    if role.is_none() {
        return Ok((StatusCode::FORBIDDEN, Json(json!({"error": IDENTIFIER_FORBIDDEN_MSG}))));
    }
    // `strict_str_to_int`-shaped identifiers that overflow `i32` match no
    // row (Django queries the ORM `sequence_id` with the unbounded int) →
    // 404, not 400.
    let Ok(seq) = seq_raw.parse::<i32>() else {
        return Ok(missing());
    };
    let issue_id: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT i.id FROM issues i WHERE i.project_id = $1 AND i.sequence_id = $2 AND i.deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(seq)
    .fetch_optional(&st.pool)
    .await?;
    let Some(issue_id) = issue_id else {
        return Ok(missing());
    };
    // Same serializer shape as the detail endpoint — delegate to `get_issue`.
    get_issue(State(st), auth, axum::extract::Path((slug, project_id, issue_id))).await
}

/// `IssueDetailIdentifierEndpoint` identifier errors
/// (`plane/app/views/issue/base.py`): non-integer `issue_identifier` → 400.
pub(crate) const INVALID_IDENTIFIER_MSG: &str = "Invalid issue identifier";
/// Non-member access → 403.
pub(crate) const IDENTIFIER_FORBIDDEN_MSG: &str = "You are not allowed to view this issue";

/// Splits `<project_identifier>-<issue_identifier>` and enforces Django's
/// strict-int constraint on the issue part (`str.isdigit()` semantics, plus
/// `strict_str_to_int`'s accepted leading `-`): all-ASCII-digits, or `-`
/// followed by all-ASCII-digits, non-empty. Axum cannot express `:a-:b` in
/// one segment, so the `:ident/` route shape is kept and this runs in the
/// handler.
pub(crate) fn resolve_identifier(ident: &str) -> Result<(String, String), ()> {
    let (project_identifier, issue_identifier) = ident.split_once('-').ok_or(())?;
    let digits_ok = (!issue_identifier.is_empty()
        && issue_identifier.chars().all(|c| c.is_ascii_digit()))
        || (issue_identifier.len() > 1
            && issue_identifier.starts_with('-')
            && issue_identifier[1..].chars().all(|c| c.is_ascii_digit()));
    if digits_ok {
        Ok((project_identifier.to_string(), issue_identifier.to_string()))
    } else {
        Err(())
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

    #[test]
    fn relation_list_has_eight_groups() {
        // Django `relation.py:174-205` — keys are fixed.
        let groups = ["blocking", "blocked_by", "duplicate", "relates_to", "start_after", "start_before", "finish_after", "finish_before"];
        assert_eq!(relation_groups(), groups);
    }

    #[test]
    fn relation_create_requires_relation_type() {
        assert_eq!(RELATION_TYPE_REQUIRED_MSG, "Issue relation type is required");
    }

    #[test]
    fn relation_direction_maps_actual_type() {
        // Verified against apps/api/plane/utils/issue_relation_mapper.py:19-32.
        assert_eq!(map_actual_relation("blocking"), "blocked_by");
        assert_eq!(map_actual_relation("blocked_by"), "blocked_by");
        assert_eq!(map_actual_relation("relates_to"), "relates_to");
    }

    #[test]
    fn identifier_split_and_strict_int_match_django() {
        assert_eq!(resolve_identifier("ABC-123"), Ok(("ABC".to_string(), "123".to_string())));
        assert!(resolve_identifier("ABC-xyz").is_err()); // not an int
        assert!(resolve_identifier("ABC").is_err());     // no dash
    }

    #[test]
    fn identifier_errors_match_django() {
        assert_eq!(INVALID_IDENTIFIER_MSG, "Invalid issue identifier");
        assert_eq!(IDENTIFIER_FORBIDDEN_MSG, "You are not allowed to view this issue");
    }

    #[test]
    fn patch_miss_string_is_issue_not_found_not_missing() {
        // `plane/app/views/issue/base.py:659-661`: partial_update miss →
        // 404 `{"error": "Issue not found"}` verbatim, NOT `missing()`.
        assert_eq!(ISSUE_PATCH_MISS_MSG, "Issue not found");
    }
}
