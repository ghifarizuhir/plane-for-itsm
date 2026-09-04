use axum::{extract::State, Json};
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
    pub count: Option<i64>,
    #[serde(default)]
    pub project_id: Option<uuid::Uuid>,
}

async fn caller(st: &AppState, auth: &AuthUser) -> Option<uuid::Uuid> {
    sqlx::query_scalar("SELECT user_id FROM api_tokens WHERE token = $1")
        .bind(&auth.0)
        .fetch_optional(&st.pool)
        .await
        .ok()
        .flatten()
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
    let user = caller(&st, &auth).await;
    let pattern = like_pattern(q.search.as_deref());
    let mut results = serde_json::Map::new();

    // Without a resolvable caller there are no memberships → empty result
    // sets (safe default; Django would 403 at permission level).
    if user.is_none() {
        for e in &entities {
            results.insert(e.clone(), Value::Array(vec![]));
        }
        return Ok(Json(Value::Object(results)));
    }
    let user = user.unwrap();

    for entity in &entities {
        let rows: Vec<Value> = match entity.as_str() {
            "workspace" => sqlx::query_as::<_, (uuid::Uuid, String, String)>(
                "SELECT w.id, w.name, w.slug FROM workspaces w JOIN workspace_members wm ON wm.workspace_id = w.id WHERE wm.member_id = $1 AND w.name ILIKE $2 AND w.deleted_at IS NULL ORDER BY w.created_at DESC LIMIT 10",
            )
            .bind(user).bind(&pattern).fetch_all(&st.pool).await?
            .into_iter().map(|(id, name, slug)| json!({"id": id, "name": name, "slug": slug})).collect(),
            "project" => sqlx::query_as::<_, (uuid::Uuid, String)>(
                "SELECT p.id, p.name FROM projects p JOIN project_members pm ON pm.project_id = p.id WHERE pm.member_id = $1 AND pm.is_active = true AND p.workspace_id = (SELECT id FROM workspaces WHERE slug = $2) AND p.name ILIKE $3 AND p.deleted_at IS NULL ORDER BY p.created_at DESC LIMIT 10",
            )
            .bind(user).bind(&slug).bind(&pattern).fetch_all(&st.pool).await?
            .into_iter().map(|(id, name)| json!({"id": id, "name": name})).collect(),
            "issue" => {
                let seqs = integer_tokens(q.search.as_deref());
                sqlx::query_as::<_, (uuid::Uuid, String)>(
                    "SELECT i.id, i.name FROM issues i JOIN project_members pm ON pm.project_id = i.project_id WHERE pm.member_id = $1 AND pm.is_active = true AND i.workspace_id = (SELECT id FROM workspaces WHERE slug = $2) AND ($3::uuid IS NULL OR i.project_id = $3) AND (i.name ILIKE $4 OR ($5::bigint[] IS NOT NULL AND i.sequence_id = ANY($5))) AND i.deleted_at IS NULL ORDER BY i.created_at DESC LIMIT 100",
                )
                .bind(user).bind(&slug).bind(q.project_id).bind(&pattern).bind(if seqs.is_empty() { None } else { Some(seqs) }).fetch_all(&st.pool).await?
                .into_iter().map(|(id, name)| json!({"id": id, "name": name})).collect()
            }
            "cycle" => sqlx::query_as::<_, (uuid::Uuid, String)>(
                "SELECT c.id, c.name FROM cycles c WHERE c.workspace_id = (SELECT id FROM workspaces WHERE slug = $1) AND ($2::uuid IS NULL OR c.project_id = $2) AND c.name ILIKE $3 AND c.deleted_at IS NULL ORDER BY c.created_at DESC LIMIT 10",
            )
            .bind(&slug).bind(q.project_id).bind(&pattern).fetch_all(&st.pool).await?
            .into_iter().map(|(id, name)| json!({"id": id, "name": name})).collect(),
            "module" => sqlx::query_as::<_, (uuid::Uuid, String)>(
                "SELECT m.id, m.name FROM modules m WHERE m.workspace_id = (SELECT id FROM workspaces WHERE slug = $1) AND ($2::uuid IS NULL OR m.project_id = $2) AND m.name ILIKE $3 AND m.deleted_at IS NULL ORDER BY m.created_at DESC LIMIT 10",
            )
            .bind(&slug).bind(q.project_id).bind(&pattern).fetch_all(&st.pool).await?
            .into_iter().map(|(id, name)| json!({"id": id, "name": name})).collect(),
            "issue_view" => sqlx::query_as::<_, (uuid::Uuid, String)>(
                "SELECT v.id, v.name FROM issue_views v WHERE v.workspace_id = (SELECT id FROM workspaces WHERE slug = $1) AND ($2::uuid IS NULL OR v.project_id = $2) AND v.name ILIKE $3 AND v.deleted_at IS NULL ORDER BY v.created_at DESC LIMIT 10",
            )
            .bind(&slug).bind(q.project_id).bind(&pattern).fetch_all(&st.pool).await?
            .into_iter().map(|(id, name)| json!({"id": id, "name": name})).collect(),
            "page" => sqlx::query_as::<_, (uuid::Uuid, String)>(
                "SELECT p.id, p.name FROM pages p LEFT JOIN project_pages pp ON pp.page_id = p.id AND pp.deleted_at IS NULL WHERE p.workspace_id = (SELECT id FROM workspaces WHERE slug = $1) AND ($2::uuid IS NULL OR pp.project_id = $2) AND p.name ILIKE $3 AND p.deleted_at IS NULL ORDER BY p.created_at DESC LIMIT 10",
            )
            .bind(&slug).bind(q.project_id).bind(&pattern).fetch_all(&st.pool).await?
            .into_iter().map(|(id, name)| json!({"id": id, "name": name})).collect(),
            "intake" => sqlx::query_as::<_, (uuid::Uuid, String)>(
                "SELECT i.id, i.name FROM intakes i WHERE i.workspace_id = (SELECT id FROM workspaces WHERE slug = $1) AND ($2::uuid IS NULL OR i.project_id = $2) AND i.name ILIKE $3 AND i.deleted_at IS NULL ORDER BY i.created_at DESC LIMIT 10",
            )
            .bind(&slug).bind(q.project_id).bind(&pattern).fetch_all(&st.pool).await?
            .into_iter().map(|(id, name)| json!({"id": id, "name": name})).collect(),
            _ => vec![],
        };
        results.insert(entity.clone(), Value::Array(rows));
    }
    Ok(Json(json!({"results": results})))
}

pub async fn issue_search(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((_slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    axum::extract::Query(q): axum::extract::Query<GlobalSearchQuery>,
) -> Result<Json<Value>, common::errors::AppError> {
    let user = caller(&st, &auth).await;
    if user.is_none() {
        return Ok(Json(json!({"results": []})));
    }
    let pattern = like_pattern(q.search.as_deref());
    let seqs = integer_tokens(q.search.as_deref());
    let rows: Vec<(uuid::Uuid, String)> = sqlx::query_as(
        "SELECT i.id, i.name FROM issues i JOIN project_members pm ON pm.project_id = i.project_id WHERE pm.member_id = $1 AND pm.is_active = true AND i.project_id = $2 AND (i.name ILIKE $3 OR ($4::bigint[] IS NOT NULL AND i.sequence_id = ANY($4))) AND i.deleted_at IS NULL ORDER BY i.created_at DESC LIMIT 100",
    )
    .bind(user.unwrap()).bind(project_id).bind(&pattern).bind(if seqs.is_empty() { None } else { Some(seqs) })
    .fetch_all(&st.pool).await?;
    Ok(Json(json!({"results": rows.into_iter().map(|(id, name)| json!({"id": id, "name": name})).collect::<Vec<_>>()})))
}

pub async fn entity_search(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    axum::extract::Query(q): axum::extract::Query<EntitySearchQuery>,
) -> Result<Json<Value>, common::errors::AppError> {
    // user_mention picker: workspace members whose user name matches.
    let pattern = like_pattern(q.query.as_deref());
    let count = q.count.unwrap_or(5).clamp(1, 50);
    let rows: Vec<(uuid::Uuid, String)> = sqlx::query_as(
        "SELECT u.id, u.display_name FROM users u JOIN workspace_members wm ON wm.member_id = u.id JOIN workspaces w ON w.id = wm.workspace_id WHERE w.slug = $1 AND ($2::uuid IS NULL OR EXISTS(SELECT 1 FROM project_members pm WHERE pm.project_id = $2 AND pm.member_id = u.id AND pm.is_active = true)) AND (u.first_name ILIKE $3 OR u.last_name ILIKE $3 OR u.display_name ILIKE $3) AND u.is_active = true ORDER BY u.display_name LIMIT $4",
    )
    .bind(&slug).bind(q.project_id).bind(&pattern).bind(count)
    .fetch_all(&st.pool).await?;
    Ok(Json(json!({"user_mention": rows.into_iter().map(|(id, display_name)| json!({"id": id, "display_name": display_name})).collect::<Vec<_>>()})))
}
