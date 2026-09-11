use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};

/// Mirrors `plane/app/views/search/` for `plane/app/urls/search.py`:
/// - `GET workspaces/:slug/search/` (GlobalSearchEndpoint):
///   `?search=&entities=&workspace_search=false&project_id=` →
///   `{"results": {workspace, project, issue, cycle, module, issue_view,
///   page, intake}}`. Unknown entities are ignored; empty/missing entities
///   searches all eight.
/// - `GET workspaces/:slug/projects/:project_id/search-issues/` →
///   issue matches for the project.
/// - `GET workspaces/:slug/entity-search/` → user-mention picker
///   (`?query=&count=5&project_id=`).
///
/// Matching mirrors the Django `icontains` filters (plus whole-integer
/// `sequence_id` match for issues). Results are membership-scoped to the
/// caller's projects like the viewsets.
///
/// STAYS ON DJANGO (`plane/app/urls/external.py`): Unsplash and GPT
/// AI-assistant endpoints — third-party API proxies needing external keys.
pub const SEARCH_ENTITIES: [&str; 8] = [
    "workspace",
    "project",
    "issue",
    "cycle",
    "module",
    "issue_view",
    "page",
    "intake",
];

pub fn parse_entities(param: Option<&str>) -> Vec<String> {
    match param {
        Some(p) => {
            let picked: Vec<String> = p
                .split(',')
                .map(str::trim)
                .filter(|e| SEARCH_ENTITIES.contains(e))
                .map(str::to_string)
                .collect();
            if picked.is_empty() {
                SEARCH_ENTITIES.iter().map(|s| s.to_string()).collect()
            } else {
                picked
            }
        }
        None => SEARCH_ENTITIES.iter().map(|s| s.to_string()).collect(),
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct GlobalSearchQuery {
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub entities: Option<String>,
    #[serde(default)]
    pub workspace_search: Option<String>,
    #[serde(default)]
    pub project_id: Option<uuid::Uuid>,
}

#[derive(Debug, Deserialize, Default)]
pub struct EntitySearchQuery {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub query_type: Option<String>,
    #[serde(default)]
    pub count: Option<i64>,
    #[serde(default)]
    pub project_id: Option<uuid::Uuid>,
}

fn like_pattern(query: Option<&str>) -> String {
    match query {
        Some(q) if !q.trim().is_empty() => format!("%{}%", q.replace(['%', '_'], "")),
        _ => "%".to_string(),
    }
}

fn integer_tokens(query: Option<&str>) -> Vec<i64> {
    match query {
        Some(q) => q
            .split(|c: char| !c.is_ascii_digit())
            .filter(|t| !t.is_empty())
            .filter_map(|t| t.parse::<i64>().ok())
            .collect(),
        None => vec![],
    }
}

pub async fn global_search(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    axum::extract::Query(q): axum::extract::Query<GlobalSearchQuery>,
) -> Result<Json<Value>, common::errors::AppError> {
    let entities = parse_entities(q.entities.as_deref());
    // AuthUser identitas sudah tervalidasi di extractor — selalu ada.
    let user = auth.0;
    let pattern = like_pattern(q.search.as_deref());
    // `workspace_search` defaults to `"false"` (`base.py:274`): the project
    // filter applies whenever `project_id` is present UNLESS
    // `workspace_search == "true"`.
    let cross_project = q.workspace_search.as_deref() == Some("true");
    let pid: Option<uuid::Uuid> = if cross_project { None } else { q.project_id };
    let mut results = serde_json::Map::new();

    for entity in &entities {
        let rows: Vec<Value> = match entity.as_str() {
            "workspace" => sqlx::query_as::<_, (uuid::Uuid, String, String)>(
                "SELECT w.id, w.name, w.slug FROM workspaces w JOIN workspace_members wm ON wm.workspace_id = w.id WHERE wm.member_id = $1 AND w.name ILIKE $2 AND w.deleted_at IS NULL ORDER BY w.created_at DESC",
            )
            .bind(user).bind(&pattern).fetch_all(&st.pool).await?
            .into_iter().map(|(id, name, slug)| json!({"id": id, "name": name, "slug": slug})).collect(),
            "project" => sqlx::query_as::<_, (uuid::Uuid, String, String, String)>(
                "SELECT p.id, p.name, p.identifier, w.slug FROM projects p JOIN workspaces w ON w.id = p.workspace_id JOIN project_members pm ON pm.project_id = p.id WHERE pm.member_id = $1 AND pm.is_active = true AND pm.deleted_at IS NULL AND w.slug = $2 AND p.archived_at IS NULL AND (p.name ILIKE $3 OR p.identifier ILIKE $3) AND p.deleted_at IS NULL ORDER BY p.created_at DESC",
            )
            .bind(user).bind(&slug).bind(&pattern).fetch_all(&st.pool).await?
            .into_iter().map(|(id, name, identifier, ws)| json!({"name": name, "id": id, "identifier": identifier, "workspace__slug": ws})).collect::<Vec<_>>(),
            "issue" => {
                let seqs = integer_tokens(q.search.as_deref());
                sqlx::query_as::<_, (uuid::Uuid, String, i32, String, uuid::Uuid, String)>(
                    "SELECT i.id, i.name, i.sequence_id, p.identifier, i.project_id, w.slug FROM issues i JOIN projects p ON p.id = i.project_id JOIN workspaces w ON w.id = i.workspace_id JOIN project_members pm ON pm.project_id = i.project_id WHERE pm.member_id = $1 AND pm.is_active = true AND pm.deleted_at IS NULL AND w.slug = $2 AND p.archived_at IS NULL AND ($3::uuid IS NULL OR i.project_id = $3) AND (i.name ILIKE $4 OR i.sequence_id::text ILIKE $4 OR p.identifier ILIKE $4 OR ($5::bigint[] IS NOT NULL AND i.sequence_id = ANY($5))) AND i.deleted_at IS NULL ORDER BY i.created_at DESC LIMIT 100",
                )
                .bind(user).bind(&slug).bind(pid).bind(&pattern).bind(if seqs.is_empty() { None } else { Some(seqs) }).fetch_all(&st.pool).await?
                .into_iter().map(|(id, name, seq, ident, proj, ws)| json!({"name": name, "id": id, "sequence_id": seq, "project__identifier": ident, "project_id": proj, "workspace__slug": ws})).collect()
            }
            "cycle" => sqlx::query_as::<_, (String, uuid::Uuid, uuid::Uuid, String, String)>(
                "SELECT c.name, c.id, c.project_id, p.identifier, w.slug FROM cycles c JOIN projects p ON p.id = c.project_id JOIN workspaces w ON w.id = c.workspace_id JOIN project_members pm ON pm.project_id = c.project_id WHERE pm.member_id = $1 AND pm.is_active = true AND pm.deleted_at IS NULL AND w.slug = $2 AND p.archived_at IS NULL AND ($3::uuid IS NULL OR c.project_id = $3) AND c.name ILIKE $4 AND c.deleted_at IS NULL ORDER BY c.created_at DESC",
            )
            .bind(user).bind(&slug).bind(pid).bind(&pattern).fetch_all(&st.pool).await?
            .into_iter().map(|(name, id, proj, ident, ws)| json!({"name": name, "id": id, "project_id": proj, "project__identifier": ident, "workspace__slug": ws})).collect(),
            "module" => sqlx::query_as::<_, (String, uuid::Uuid, uuid::Uuid, String, String)>(
                "SELECT m.name, m.id, m.project_id, p.identifier, w.slug FROM modules m JOIN projects p ON p.id = m.project_id JOIN workspaces w ON w.id = m.workspace_id JOIN project_members pm ON pm.project_id = m.project_id WHERE pm.member_id = $1 AND pm.is_active = true AND pm.deleted_at IS NULL AND w.slug = $2 AND p.archived_at IS NULL AND ($3::uuid IS NULL OR m.project_id = $3) AND m.name ILIKE $4 AND m.deleted_at IS NULL ORDER BY m.created_at DESC",
            )
            .bind(user).bind(&slug).bind(pid).bind(&pattern).fetch_all(&st.pool).await?
            .into_iter().map(|(name, id, proj, ident, ws)| json!({"name": name, "id": id, "project_id": proj, "project__identifier": ident, "workspace__slug": ws})).collect(),
            "issue_view" => sqlx::query_as::<_, (String, uuid::Uuid, uuid::Uuid, String, String)>(
                "SELECT v.name, v.id, v.project_id, p.identifier, w.slug FROM issue_views v JOIN projects p ON p.id = v.project_id JOIN workspaces w ON w.id = v.workspace_id JOIN project_members pm ON pm.project_id = v.project_id WHERE pm.member_id = $1 AND pm.is_active = true AND pm.deleted_at IS NULL AND w.slug = $2 AND p.archived_at IS NULL AND ($3::uuid IS NULL OR v.project_id = $3) AND v.name ILIKE $4 AND v.deleted_at IS NULL ORDER BY v.created_at DESC",
            )
            .bind(user).bind(&slug).bind(pid).bind(&pattern).fetch_all(&st.pool).await?
            .into_iter().map(|(name, id, proj, ident, ws)| json!({"name": name, "id": id, "project_id": proj, "project__identifier": ident, "workspace__slug": ws})).collect(),
            "page" => {
                // `filter_pages`: member projects + `project_ids` /
                // `project_identifiers` arrays; project-scoped variant
                // annotates the single `project_id` instead.
                if let Some(p) = pid {
                    sqlx::query_as::<_, (String, uuid::Uuid, uuid::Uuid, String)>(
                        "SELECT DISTINCT p.name, p.id, w.slug, $3 FROM pages p LEFT JOIN project_pages pp ON pp.page_id = p.id AND pp.deleted_at IS NULL JOIN workspaces w ON w.id = p.workspace_id JOIN project_members pm ON pm.project_id = pp.project_id WHERE pm.member_id = $1 AND pm.is_active = true AND pm.deleted_at IS NULL AND w.slug = $2 AND pp.project_id = $3 AND p.name ILIKE $4 AND p.deleted_at IS NULL ORDER BY p.created_at DESC",
                    )
                    .bind(user).bind(&slug).bind(p).bind(&pattern).fetch_all(&st.pool).await?
                    .into_iter().map(|(name, id, ws, proj)| json!({"name": name, "id": id, "project_id": proj, "workspace__slug": ws})).collect()
                } else {
                    sqlx::query_as::<_, (String, uuid::Uuid, Vec<uuid::Uuid>, Vec<String>, String)>(
                        "SELECT p.name, p.id, COALESCE(ARRAY(SELECT DISTINCT pp2.project_id FROM project_pages pp2 WHERE pp2.page_id = p.id AND pp2.deleted_at IS NULL), '{}'), COALESCE(ARRAY(SELECT DISTINCT pr.identifier FROM project_pages pp2 JOIN projects pr ON pr.id = pp2.project_id WHERE pp2.page_id = p.id AND pp2.deleted_at IS NULL), '{}'), w.slug FROM pages p JOIN workspaces w ON w.id = p.workspace_id WHERE w.slug = $2 AND p.name ILIKE $3 AND p.deleted_at IS NULL AND EXISTS(SELECT 1 FROM project_pages pp JOIN project_members pm ON pm.project_id = pp.project_id WHERE pp.page_id = p.id AND pp.deleted_at IS NULL AND pm.member_id = $1 AND pm.is_active = true AND pm.deleted_at IS NULL) ORDER BY p.created_at DESC",
                    )
                    .bind(user).bind(&slug).bind(&pattern).fetch_all(&st.pool).await?
                    .into_iter().map(|(name, id, pids, idents, ws)| json!({"name": name, "id": id, "project_ids": pids, "project_identifiers": idents, "workspace__slug": ws})).collect()
                }
            }
            "intake" => {
                // `filter_intakes`: INTAKE issues (status Snoozed=0 or
                // Pending=-2), issue keys, [:100].
                let seqs = integer_tokens(q.search.as_deref());
                sqlx::query_as::<_, (String, uuid::Uuid, i32, String, uuid::Uuid, String)>(
                    "SELECT i.name, i.id, i.sequence_id, p.identifier, i.project_id, w.slug FROM issues i JOIN projects p ON p.id = i.project_id JOIN workspaces w ON w.id = i.workspace_id JOIN project_members pm ON pm.project_id = i.project_id JOIN intake_issues iti ON iti.issue_id = i.id AND iti.status IN (0, -2) AND iti.deleted_at IS NULL WHERE pm.member_id = $1 AND pm.is_active = true AND pm.deleted_at IS NULL AND w.slug = $2 AND p.archived_at IS NULL AND ($3::uuid IS NULL OR i.project_id = $3) AND (i.name ILIKE $4 OR i.sequence_id::text ILIKE $4 OR p.identifier ILIKE $4 OR ($5::bigint[] IS NOT NULL AND i.sequence_id = ANY($5))) AND i.deleted_at IS NULL ORDER BY i.created_at DESC LIMIT 100",
                )
                .bind(user).bind(&slug).bind(pid).bind(&pattern).bind(if seqs.is_empty() { None } else { Some(seqs) }).fetch_all(&st.pool).await?
                .into_iter().map(|(name, id, seq, ident, proj, ws)| json!({"name": name, "id": id, "sequence_id": seq, "project__identifier": ident, "project_id": proj, "workspace__slug": ws})).collect()
            }
            _ => vec![],
        };
        results.insert(entity.clone(), Value::Array(rows));
    }
    Ok(Json(json!({"results": results})))
}

/// One row of the project issue-search result, mirroring Django
/// `IssueSearchEndpoint.get` `.values(...)` keys
/// (`plane/app/views/search/issue.py:146-160`).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SearchIssueRow {
    pub id: uuid::Uuid,
    pub name: String,
    pub start_date: Option<chrono::NaiveDate>,
    pub sequence_id: i32,
    pub project__name: String,
    pub project__identifier: String,
    pub project_id: uuid::Uuid,
    pub workspace__slug: String,
    pub state__name: Option<String>,
    pub state__group: Option<String>,
    pub state__color: Option<String>,
}

/// Maps a [`SearchIssueRow`] to the bare-array item the web modals consume
/// (`ISearchIssueResponse`). Extracted so `search_test` covers the shape
/// without a live DB.
pub fn build_search_issue_item(row: SearchIssueRow) -> Value {
    json!({
        "id": row.id,
        "name": row.name,
        "start_date": row.start_date,
        "sequence_id": row.sequence_id,
        "project__name": row.project__name,
        "project__identifier": row.project__identifier,
        "project_id": row.project_id,
        "workspace__slug": row.workspace__slug,
        "state__name": row.state__name,
        "state__group": row.state__group,
        "state__color": row.state__color,
    })
}

pub async fn issue_search(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((_slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    axum::extract::Query(q): axum::extract::Query<GlobalSearchQuery>,
) -> Result<Json<Value>, common::errors::AppError> {
    let user = auth.0;
    let pattern = like_pattern(q.search.as_deref());
    let seqs = integer_tokens(q.search.as_deref());
    // Django `IssueSearchEndpoint.get` (`plane/app/views/search/issue.py`)
    // returns a BARE array via `Response(issues.values(...))` — NOT a
    // `{"results": [...]}` envelope. `projectIssuesSearch` passes
    // `response?.data` straight into `setIssues`, so an envelope object
    // crashed every consumer (`issues.map is not a function`).
    let rows: Vec<SearchIssueRow> = sqlx::query_as(
        "SELECT i.id, i.name, i.start_date, i.sequence_id, p.name AS project__name, p.identifier AS project__identifier, i.project_id, w.slug AS workspace__slug, s.name AS state__name, s.\"group\" AS state__group, s.color AS state__color FROM issues i JOIN projects p ON p.id = i.project_id JOIN workspaces w ON w.id = i.workspace_id LEFT JOIN states s ON s.id = i.state_id JOIN project_members pm ON pm.project_id = i.project_id WHERE pm.member_id = $1 AND pm.is_active = true AND i.project_id = $2 AND (i.name ILIKE $3 OR ($4::bigint[] IS NOT NULL AND i.sequence_id = ANY($4))) AND i.deleted_at IS NULL ORDER BY i.created_at DESC LIMIT 100",
    )
    .bind(user).bind(project_id).bind(&pattern).bind(if seqs.is_empty() { None } else { Some(seqs) })
    .fetch_all(&st.pool).await?;
    let items: Vec<Value> = rows.into_iter().map(build_search_issue_item).collect();
    Ok(Json(Value::Array(items)))
}

pub async fn entity_search(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    axum::extract::Query(q): axum::extract::Query<EntitySearchQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    use crate::routes::member::deny_detail;
    use crate::routes::project::ws_role;
    // `WorkspaceUserPermission` (`base.py:306`): any active member.
    if ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(deny_detail());
    }
    let user = auth.0;
    let pattern = like_pattern(q.query.as_deref());
    // `count = int(...default 5)`: no upper clamp in Django; negatives slice
    // to [] there — clamp the floor at 0 here (sane-mapping).
    let count = q.count.unwrap_or(5).max(0);
    let types: Vec<&str> = q
        .query_type
        .as_deref()
        .map(|s| s.split(',').map(str::trim).collect())
        .unwrap_or_else(|| vec!["user_mention"]);
    let mut out = serde_json::Map::new();
    for qt in types {
        let rows: Vec<Value> = match qt {
            "user_mention" => {
                // Project branch: project members; else workspace members
                // (`base.py:322-365,540-580`). Keys keep the Django
                // `member__` annotation names.
                if let Some(pid) = q.project_id {
                    sqlx::query_as::<_, (Option<String>, Option<String>, Option<uuid::Uuid>)>(
                        "SELECT CASE WHEN u.avatar_asset_id IS NOT NULL \
                          THEN '/api/assets/v2/static/' || u.avatar_asset_id::text || '/' ELSE u.avatar END, \
                         u.display_name, u.id FROM users u \
                         JOIN project_members pm ON pm.member_id = u.id \
                         WHERE pm.project_id = $1 AND pm.is_active = true AND pm.deleted_at IS NULL \
                         AND u.is_bot = false AND (u.first_name ILIKE $2 OR u.last_name ILIKE $2 OR u.display_name ILIKE $2) \
                         ORDER BY pm.created_at DESC LIMIT $3",
                    )
                    .bind(pid).bind(&pattern).bind(count).fetch_all(&st.pool).await?
                    .into_iter().map(|(a, d, i)| json!({"member__avatar_url": a, "member__display_name": d, "member__id": i})).collect()
                } else {
                    sqlx::query_as::<_, (Option<String>, Option<String>, Option<uuid::Uuid>)>(
                        "SELECT CASE WHEN u.avatar_asset_id IS NOT NULL \
                          THEN '/api/assets/v2/static/' || u.avatar_asset_id::text || '/' ELSE u.avatar END, \
                         u.display_name, u.id FROM users u \
                         JOIN workspace_members wm ON wm.member_id = u.id \
                         JOIN workspaces w ON w.id = wm.workspace_id \
                         WHERE w.slug = $1 AND wm.is_active = true AND wm.deleted_at IS NULL \
                         AND u.is_bot = false AND (u.first_name ILIKE $2 OR u.last_name ILIKE $2 OR u.display_name ILIKE $2) \
                         ORDER BY wm.created_at DESC LIMIT $3",
                    )
                    .bind(&slug).bind(&pattern).bind(count).fetch_all(&st.pool).await?
                    .into_iter().map(|(a, d, i)| json!({"member__avatar_url": a, "member__display_name": d, "member__id": i})).collect()
                }
            }
            "project" => {
                // Member projects OR public (`base.py:377-390,591-604`).
                sqlx::query_as::<_, (String, uuid::Uuid, String, Value, String)>(
                    "SELECT p.name, p.id, p.identifier, p.logo_props, w.slug FROM projects p \
                     JOIN workspaces w ON w.id = p.workspace_id \
                     WHERE w.slug = $1 AND (p.name ILIKE $2 OR p.identifier ILIKE $2) \
                     AND (EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = p.id \
                       AND pm.member_id = $3 AND pm.deleted_at IS NULL) OR p.network = 2) \
                     AND p.deleted_at IS NULL ORDER BY p.created_at DESC LIMIT $4",
                )
                .bind(&slug).bind(&pattern).bind(user).bind(count).fetch_all(&st.pool).await?
                .into_iter().map(|(name, id, ident, logo, ws)| json!({"name": name, "id": id, "identifier": ident, "logo_props": logo, "workspace__slug": ws})).collect()
            }
            "issue" => {
                let seqs = integer_tokens(q.query.as_deref());
                sqlx::query_as::<_, (String, uuid::Uuid, i32, String, uuid::Uuid, String, Option<uuid::Uuid>, Option<uuid::Uuid>)>(
                    "SELECT i.name, i.id, i.sequence_id, p.identifier, i.project_id, i.priority, i.state_id, i.type_id FROM issues i \
                     JOIN projects p ON p.id = i.project_id JOIN workspaces w ON w.id = i.workspace_id \
                     JOIN project_members pm ON pm.project_id = i.project_id \
                     WHERE w.slug = $1 AND ($2::uuid IS NULL OR i.project_id = $2) \
                     AND pm.member_id = $3 AND pm.is_active = true AND pm.deleted_at IS NULL \
                     AND (i.name ILIKE $4 OR p.identifier ILIKE $4 OR ($5::bigint[] IS NOT NULL AND i.sequence_id = ANY($5))) \
                     AND i.deleted_at IS NULL ORDER BY i.created_at DESC LIMIT $6",
                )
                .bind(&slug).bind(q.project_id).bind(user).bind(&pattern).bind(if seqs.is_empty() { None } else { Some(seqs) }).bind(count).fetch_all(&st.pool).await?
                .into_iter().map(|(name, id, seq, ident, proj, prio, sid, tid)| json!({"name": name, "id": id, "sequence_id": seq, "project__identifier": ident, "project_id": proj, "priority": prio, "state_id": sid, "type_id": tid})).collect()
            }
            "cycle" => {
                sqlx::query_as::<_, (String, uuid::Uuid, uuid::Uuid, String, String, String)>(
                    "SELECT c.name, c.id, c.project_id, p.identifier, \
                     CASE WHEN c.start_date <= CURRENT_DATE AND c.end_date >= CURRENT_DATE THEN 'CURRENT' \
                          WHEN c.start_date > CURRENT_DATE THEN 'UPCOMING' \
                          WHEN c.end_date < CURRENT_DATE THEN 'COMPLETED' \
                          WHEN c.start_date IS NULL AND c.end_date IS NULL THEN 'DRAFT' \
                          ELSE 'DRAFT' END, \
                     w.slug FROM cycles c JOIN projects p ON p.id = c.project_id \
                     JOIN workspaces w ON w.id = c.workspace_id \
                     JOIN project_members pm ON pm.project_id = c.project_id \
                     WHERE w.slug = $1 AND ($2::uuid IS NULL OR c.project_id = $2) \
                     AND pm.member_id = $3 AND pm.is_active = true AND pm.deleted_at IS NULL \
                     AND c.name ILIKE $4 AND c.deleted_at IS NULL ORDER BY c.created_at DESC LIMIT $5",
                )
                .bind(&slug).bind(q.project_id).bind(user).bind(&pattern).bind(count).fetch_all(&st.pool).await?
                .into_iter().map(|(name, id, proj, ident, status, ws)| json!({"name": name, "id": id, "project_id": proj, "project__identifier": ident, "status": status, "workspace__slug": ws})).collect()
            }
            "module" => {
                sqlx::query_as::<_, (String, uuid::Uuid, uuid::Uuid, String, String, String)>(
                    "SELECT m.name, m.id, m.project_id, p.identifier, m.status, w.slug FROM modules m \
                     JOIN projects p ON p.id = m.project_id JOIN workspaces w ON w.id = m.workspace_id \
                     JOIN project_members pm ON pm.project_id = m.project_id \
                     WHERE w.slug = $1 AND ($2::uuid IS NULL OR m.project_id = $2) \
                     AND pm.member_id = $3 AND pm.is_active = true AND pm.deleted_at IS NULL \
                     AND m.name ILIKE $4 AND m.deleted_at IS NULL ORDER BY m.created_at DESC LIMIT $5",
                )
                .bind(&slug).bind(q.project_id).bind(user).bind(&pattern).bind(count).fetch_all(&st.pool).await?
                .into_iter().map(|(name, id, proj, ident, status, ws)| json!({"name": name, "id": id, "project_id": proj, "project__identifier": ident, "status": status, "workspace__slug": ws})).collect()
            }
            "page" => {
                // Project branch: single `projects__id`; global branch:
                // public + global pages, one row per project link
                // (`base.py:505-530,700-729`).
                if let Some(pid) = q.project_id {
                    sqlx::query_as::<_, (String, uuid::Uuid, Value, uuid::Uuid, String)>(
                        "SELECT DISTINCT p.name, p.id, p.logo_props, pp.project_id, w.slug FROM pages p \
                         JOIN workspaces w ON w.id = p.workspace_id \
                         JOIN project_pages pp ON pp.page_id = p.id AND pp.deleted_at IS NULL \
                         JOIN project_members pm ON pm.project_id = pp.project_id \
                         WHERE w.slug = $1 AND pp.project_id = $2 AND pm.member_id = $3 \
                         AND pm.is_active = true AND pm.deleted_at IS NULL \
                         AND p.name ILIKE $4 AND p.access = 0 AND p.deleted_at IS NULL \
                         ORDER BY p.created_at DESC LIMIT $5",
                    )
                    .bind(&slug).bind(pid).bind(user).bind(&pattern).bind(count).fetch_all(&st.pool).await?
                    .into_iter().map(|(name, id, logo, proj, ws)| json!({"name": name, "id": id, "logo_props": logo, "projects__id": proj, "workspace__slug": ws})).collect()
                } else {
                    sqlx::query_as::<_, (String, uuid::Uuid, Value, uuid::Uuid, String)>(
                        "SELECT DISTINCT p.name, p.id, p.logo_props, pp.project_id, w.slug FROM pages p \
                         JOIN workspaces w ON w.id = p.workspace_id \
                         JOIN project_pages pp ON pp.page_id = p.id AND pp.deleted_at IS NULL \
                         JOIN project_members pm ON pm.project_id = pp.project_id \
                         WHERE w.slug = $1 AND pm.member_id = $2 AND pm.is_active = true \
                         AND pm.deleted_at IS NULL AND p.name ILIKE $3 AND p.access = 0 \
                         AND p.is_global = true AND p.deleted_at IS NULL \
                         ORDER BY p.created_at DESC LIMIT $4",
                    )
                    .bind(&slug).bind(user).bind(&pattern).bind(count).fetch_all(&st.pool).await?
                    .into_iter().map(|(name, id, logo, proj, ws)| json!({"name": name, "id": id, "logo_props": logo, "projects__id": proj, "workspace__slug": ws})).collect()
                }
            }
            _ => continue,
        };
        out.insert(qt.to_string(), Value::Array(rows));
    }
    Ok((StatusCode::OK, Json(Value::Object(out))))
}
