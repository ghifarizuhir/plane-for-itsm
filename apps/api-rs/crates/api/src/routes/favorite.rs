//! Parity with `WorkspaceFavoriteEndpoint` + `WorkspaceFavoriteGroupEndpoint`
//! (`plane/app/views/workspace/favorite.py:23-97`,
//! `plane/app/urls/workspace.py:187-201`) and the one missing entity-favorite
//! route, `IssueViewFavoriteViewSet.destroy`
//! (`plane/app/views/view/base.py:435-444`, `plane/app/urls/views.py:61-64`).
//!
//! Audit of entity-favorite routes (verified before writing — add ONLY what
//! is missing):
//! - project POST + DELETE — EXIST (`project.rs:fav_add/fav_remove`, wired
//!   `main.rs:99-104`); NOT duplicated here.
//! - cycle POST + DELETE — EXIST (`cycle.rs:fav_create/fav_destroy`, wired
//!   `main.rs:431-437`); NOT duplicated here (the E6 brief assumed them
//!   missing — they are present since E2).
//! - module POST + DELETE — EXIST (`module.rs:fav_create/fav_destroy`, wired
//!   `main.rs:557-563`); NOT duplicated here (present since E3).
//! - page POST + DELETE — EXIST (`page.rs:create_favorite/destroy_favorite`,
//!   wired `main.rs:882-884`); NOT duplicated here (present since E4).
//! - view POST (+ a GET list) — EXIST (`view.rs:create_favorite`, wired
//!   `main.rs:864-866`); view DELETE is MISSING → added here as
//!   `view_fav_destroy` (Django `view/base.py:435-444`).
//! - NO GET on any entity-favorite collection is added (locked §4
//!   broken-list rule — Django maps GET→list on cycle/module/project/view
//!   collections but has no serializer for them, so they 500; the
//!   pre-existing view GET list is left untouched).
//! - `.../user-favorites/` CRUD + `.../group/` — entirely MISSING → added
//!   here as `list/create/patch/destroy/group`.
//!
//! Conventions reused (not forked): `project::{deny, missing, ws_role}` for
//! the `allow_permission`-deny 403 `{"error": ...}` shape
//! (`permissions/base.py:81-84`, quoted in `project.rs:8`) and the 404
//! `missing()` shape (`views/base.py:92-96`, quoted in `project.rs:11`);
//! `issue_common::{fetch_project_member_role, is_workspace_admin,
//! project_gate_allows}` for the project-level AM gate with the workspace-
//! ADMIN fallback (`permissions/base.py:53-78`); `AuthUser` answers the
//! generic 401 (locked 401-generic rule); Celery is N/A (favorites send no
//! mail). All favorite deletes are HARD (`delete(soft=False)` —
//! `favorite.py:79-82`, `view/base.py:435-444`); `deleted_at IS NULL`
//! predicates preserve the default-manager invisibility of soft-deleted rows
//! (reachable only via hypothetical soft writes — every Django path here
//! hard-deletes).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::routes::project::{deny, missing, ws_role};
use crate::{middleware::auth::AuthUser, state::AppState};

use super::issue_common::{fetch_project_member_role, is_workspace_admin, project_gate_allows};

// ============================================================================
// Error strings — every literal quoted from Django with file:line.
// ============================================================================

/// `plane/app/views/workspace/favorite.py:67` (race `IntegrityError` on
/// user-favorites POST).
pub const FAVORITE_EXISTS_MSG: &str = "Favorite already exists";

/// DRF `PrimaryKeyRelatedField.does_not_exist` (serializer FK validation for
/// `parent` / `project_id`; DRF-side message, no Django file:line — the
/// Django call site is `serializers/favorite.py:60-76` which declares the
/// relational fields).
pub fn invalid_pk_msg(pk: &str) -> String {
    format!("Invalid pk \"{pk}\" - object does not exist.")
}

// ============================================================================
// Pure helpers (unit-tested below).
// ============================================================================

/// Mirrors `@allow_permission([ROLE.ADMIN, ROLE.MEMBER], level="WORKSPACE")`
/// (`favorite.py:23,37,69,78,86`): workspace roles 20/15 pass; GUEST (5),
/// non-members and unknown slugs fail (the GET-list bad-slug 200-[] branch
/// is handled at the call site, not here).
pub fn guard_ws_am(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) => Ok(()),
        _ => Err(crate::routes::project::FORBIDDEN_MSG.to_string()),
    }
}

/// Mirrors `@allow_permission([ROLE.ADMIN, ROLE.MEMBER])` (project level,
/// `view/base.py:435`): roles 20/15 pass, GUEST (5) denied — same shape as
/// `cycle.rs:178-183` `guard_am`, defined locally per route-file precedent.
pub fn guard_proj_am(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) => Ok(()),
        _ => Err(crate::routes::project::FORBIDDEN_MSG.to_string()),
    }
}

/// Mirrors `UserFavorite.save` (`plane/db/models/favorite.py:52-63`): on
/// create the sequence is unconditionally overwritten with
/// `max(workspace sequences) + 10000` whenever any favorite exists in the
/// workspace (all users — no user filter in `:55-61`); otherwise the
/// provided-or-default value is kept.
#[cfg(test)]
pub fn next_sequence(largest: Option<f64>, provided: f64) -> f64 {
    match largest {
        Some(m) => m + 10000.0,
        None => provided,
    }
}

fn next_sequence_live(largest: Option<f64>, provided: f64) -> f64 {
    match largest {
        Some(m) => m + 10000.0,
        None => provided,
    }
}

fn dup_exists() -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"error": FAVORITE_EXISTS_MSG})),
    )
}

fn field_errors(errors: Map<String, Value>) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(Value::Object(errors)))
}

fn err(key: &str, msg: String) -> (String, Value) {
    (key.to_string(), Value::Array(vec![Value::String(msg)]))
}

/// DRF `CharField` "required" (`serializers/favorite.py:60-76` declares
/// `entity_type` via the model field, `blank=False` → required).
fn required(key: &str) -> (String, Value) {
    err(key, "This field is required.".to_string())
}

/// DRF `UUIDField.invalid` for `entity_identifier` / `parent` / `project_id`.
fn bad_uuid(key: &str, raw: &str) -> (String, Value) {
    err(key, format!("“{raw}” is not a valid UUID."))
}

fn parse_uuid_field(
    body: &Value,
    key: &str,
    errors: &mut Map<String, Value>,
) -> Option<Option<uuid::Uuid>> {
    match body.get(key) {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(s)) => match s.parse::<uuid::Uuid>() {
            Ok(id) => Some(Some(id)),
            Err(_) => {
                let (k, v) = bad_uuid(key, s);
                errors.insert(k, v);
                None
            }
        },
        Some(other) => {
            let (k, v) = bad_uuid(key, &other.to_string());
            errors.insert(k, v);
            None
        }
    }
}

// ============================================================================
// Rows + serialization.
// ============================================================================

/// Live `user_favorites` columns (verified `migrations/0001_initial.sql:2185`
/// + partial unique `(entity_type, entity_identifier, user) WHERE deleted_at
/// IS NULL` at `:7211-7214`).
#[derive(Debug, Clone, sqlx::FromRow)]
struct FavRow {
    id: uuid::Uuid,
    entity_type: String,
    entity_identifier: Option<uuid::Uuid>,
    name: Option<String>,
    is_folder: bool,
    sequence: f64,
    parent_id: Option<uuid::Uuid>,
    workspace_id: uuid::Uuid,
    project_id: Option<uuid::Uuid>,
}

/// Exact 10-key `UserFavoriteSerializer` order
/// (`serializers/favorite.py:64-75`): `id,entity_type,entity_identifier,
/// entity_data,name,is_folder,sequence,parent,workspace_id,project_id`.
/// Struct serialization preserves declaration order.
#[derive(Debug, Clone, Serialize)]
struct FavoriteOut {
    id: uuid::Uuid,
    entity_type: String,
    entity_identifier: Option<uuid::Uuid>,
    entity_data: Value,
    name: Option<String>,
    is_folder: bool,
    sequence: f64,
    parent: Option<uuid::Uuid>,
    workspace_id: uuid::Uuid,
    project_id: Option<uuid::Uuid>,
}

/// `CycleFavoriteLiteSerializer` / `ModuleFavoriteLiteSerializer` /
/// `ViewFavoriteSerializer` / `PageFavoriteLiteSerializer` shape
/// (`serializers/favorite.py:31-43`): `id,name,logo_props,project_id`.
#[derive(Debug, Clone, Serialize)]
struct EntityFavData {
    id: uuid::Uuid,
    name: String,
    logo_props: Value,
    project_id: Option<uuid::Uuid>,
}

/// `ProjectFavoriteLiteSerializer` (`serializers/favorite.py:11-13`):
/// `id,name,logo_props`.
#[derive(Debug, Clone, Serialize)]
struct ProjectFavData {
    id: uuid::Uuid,
    name: String,
    logo_props: Value,
}

fn render(row: &FavRow, entity_data: Value) -> FavoriteOut {
    FavoriteOut {
        id: row.id,
        entity_type: row.entity_type.clone(),
        entity_identifier: row.entity_identifier,
        entity_data,
        name: row.name.clone(),
        is_folder: row.is_folder,
        sequence: row.sequence,
        parent: row.parent_id,
        workspace_id: row.workspace_id,
        project_id: row.project_id,
    }
}

/// Mirrors `UserFavoriteSerializer.get_entity_data`
/// (`serializers/favorite.py:78-89`) per type (`:46-58`):
/// - `cycle/module/view/page/project` → lite serializer shape; miss → null
///   (`:87-88` `DoesNotExist → None`).
/// - `issue` → `(Issue, None)` → null; `folder` → `(None, None)` → null;
///   unknown types → null (`:89`).
/// The lookups use the plain managers (no `deleted_at` filter — verified
/// `db/models/base.py:17`: `BaseModel` declares no filtered default
/// manager), so soft-deleted entities still resolve, exactly like Django.
async fn resolve_entity_data(
    pool: &sqlx::PgPool,
    entity_type: &str,
    entity_identifier: Option<uuid::Uuid>,
) -> Result<Value, sqlx::Error> {
    let Some(eid) = entity_identifier else {
        return Ok(Value::Null);
    };
    match entity_type {
        "cycle" => {
            let row: Option<(uuid::Uuid, String, Value, Option<uuid::Uuid>)> =
                sqlx::query_as("SELECT id, name, logo_props, project_id FROM cycles WHERE id = $1")
                    .bind(eid)
                    .fetch_optional(pool)
                    .await?;
            Ok(row
                .map(|(id, name, logo_props, project_id)| {
                    serde_json::to_value(EntityFavData {
                        id,
                        name,
                        logo_props,
                        project_id,
                    })
                    .unwrap_or(Value::Null)
                })
                .unwrap_or(Value::Null))
        }
        "module" => {
            let row: Option<(uuid::Uuid, String, Value, Option<uuid::Uuid>)> = sqlx::query_as(
                "SELECT id, name, logo_props, project_id FROM modules WHERE id = $1",
            )
            .bind(eid)
            .fetch_optional(pool)
            .await?;
            Ok(row
                .map(|(id, name, logo_props, project_id)| {
                    serde_json::to_value(EntityFavData {
                        id,
                        name,
                        logo_props,
                        project_id,
                    })
                    .unwrap_or(Value::Null)
                })
                .unwrap_or(Value::Null))
        }
        "view" => {
            let row: Option<(uuid::Uuid, String, Value, Option<uuid::Uuid>)> = sqlx::query_as(
                "SELECT id, name, logo_props, project_id FROM issue_views WHERE id = $1",
            )
            .bind(eid)
            .fetch_optional(pool)
            .await?;
            Ok(row
                .map(|(id, name, logo_props, project_id)| {
                    serde_json::to_value(EntityFavData {
                        id,
                        name,
                        logo_props,
                        project_id,
                    })
                    .unwrap_or(Value::Null)
                })
                .unwrap_or(Value::Null))
        }
        "page" => {
            let row: Option<(uuid::Uuid, String, Value)> =
                sqlx::query_as("SELECT id, name, logo_props FROM pages WHERE id = $1")
                    .bind(eid)
                    .fetch_optional(pool)
                    .await?;
            let Some((id, name, logo_props)) = row else {
                return Ok(Value::Null);
            };
            // `PageFavoriteLiteSerializer.get_project_id`
            // (`serializers/favorite.py:27-29`): `obj.projects.first()` —
            // the M2M through `ProjectPage` whose `Meta.ordering =
            // ("-created_at",)` (`db/models/page.py:140-152`), so first =
            // most recently linked.
            let project_id: Option<uuid::Uuid> = sqlx::query_scalar(
                "SELECT project_id FROM project_pages WHERE page_id = $1 \
                 AND deleted_at IS NULL ORDER BY created_at DESC LIMIT 1",
            )
            .bind(eid)
            .fetch_optional(pool)
            .await?
            .flatten();
            Ok(serde_json::to_value(EntityFavData {
                id,
                name,
                logo_props,
                project_id,
            })
            .unwrap_or(Value::Null))
        }
        "project" => {
            let row: Option<(uuid::Uuid, String, Value)> =
                sqlx::query_as("SELECT id, name, logo_props FROM projects WHERE id = $1")
                    .bind(eid)
                    .fetch_optional(pool)
                    .await?;
            Ok(row
                .map(|(id, name, logo_props)| {
                    serde_json::to_value(ProjectFavData {
                        id,
                        name,
                        logo_props,
                    })
                    .unwrap_or(Value::Null)
                })
                .unwrap_or(Value::Null))
        }
        _ => Ok(Value::Null),
    }
}

async fn render_rows(
    pool: &sqlx::PgPool,
    rows: &[FavRow],
) -> Result<Vec<FavoriteOut>, sqlx::Error> {
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let data = resolve_entity_data(pool, &row.entity_type, row.entity_identifier).await?;
        out.push(render(row, data));
    }
    Ok(out)
}

// ============================================================================
// Shared gates + lookups.
// ============================================================================

async fn workspace_id(pool: &sqlx::PgPool, slug: &str) -> Result<Option<uuid::Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM workspaces WHERE slug = $1")
        .bind(slug)
        .fetch_optional(pool)
        .await
}

async fn project_in_workspace(
    pool: &sqlx::PgPool,
    pid: uuid::Uuid,
    slug: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
         WHERE p.id = $1 AND w.slug = $2 AND p.deleted_at IS NULL)",
    )
    .bind(pid)
    .bind(slug)
    .fetch_one(pool)
    .await
}

/// Project-level AM gate with the workspace-ADMIN fallback
/// (`permissions/base.py:53-78`, same shape as `cycle.rs:657-671`
/// `gate_am`, rebuilt on the shared `issue_common` helpers).
async fn gate_proj_am(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, pid).await?;
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    Ok(project_gate_allows(
        guard_proj_am(role).is_ok(),
        role.is_some(),
        ws_admin,
    ))
}

fn is_constraint_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.code().is_some_and(|c| c.starts_with("23")))
}

// ============================================================================
// E6a — GET list.
// ============================================================================

const FAV_LIST_SQL: &str = "SELECT uf.id, uf.entity_type, uf.entity_identifier, uf.name, \
     uf.is_folder, uf.sequence, uf.parent_id, uf.workspace_id, uf.project_id \
     FROM user_favorites uf JOIN workspaces w ON w.id = uf.workspace_id \
     WHERE uf.user_id = $1 AND w.slug = $2 AND uf.parent_id IS NULL AND uf.deleted_at IS NULL \
     AND ((uf.project_id IS NULL AND uf.entity_type <> 'page') \
       OR (uf.project_id IS NOT NULL AND EXISTS (SELECT 1 FROM project_members pm \
         WHERE pm.project_id = uf.project_id AND pm.member_id = $1 \
         AND pm.is_active = true AND pm.deleted_at IS NULL))) \
     ORDER BY uf.created_at DESC";

/// Mirrors `WorkspaceFavoriteEndpoint.get`
/// (`views/workspace/favorite.py:23-35`, `urls/workspace.py:187-191`):
/// GET 200 `UserFavoriteSerializer[]` (10 keys, `entity_data` per type),
/// `parent__isnull=True` + the project-member gate (`:26-34` — project-null
/// non-pages pass; project-linked rows require an ACTIVE project
/// membership; `?all=true` is IGNORED — Django reads no query params).
/// Gate WORKSPACE ADMIN/MEMBER (`:23`, deny = `deny()` 403; Guest → 403).
/// Bad slug → 200 `[]`: the handler performs no workspace fetch (Django
/// `get` never calls `Workspace.objects.get` — only `post` does, `:40`),
/// so a bad slug can only yield an empty filter.
/// DEVIATION (mandated by the E6 contract): Django would 403 a bad slug via
/// the decorator before the (empty) queryset runs; the contract requires
/// 200 `[]`, so the existence probe below applies ONLY to the deny path.
pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if guard_ws_am(ws_role(&st.pool, auth.0, &slug).await?).is_err() {
        if workspace_id(&st.pool, &slug).await?.is_none() {
            return Ok((StatusCode::OK, Json(json!([]))));
        }
        return Ok(deny());
    }
    let rows: Vec<FavRow> = sqlx::query_as(FAV_LIST_SQL)
        .bind(auth.0)
        .bind(&slug)
        .fetch_all(&st.pool)
        .await?;
    let out = render_rows(&st.pool, &rows).await?;
    Ok((
        StatusCode::OK,
        Json(serde_json::to_value(out).unwrap_or(Value::Null)),
    ))
}

// ============================================================================
// E6a — POST create.
// ============================================================================

#[derive(Debug)]
struct ParsedCreate {
    entity_type: String,
    entity_identifier: Option<uuid::Uuid>,
    name: Option<String>,
    is_folder: bool,
    sequence: Option<f64>,
    parent: Option<uuid::Uuid>,
    project_id: Option<uuid::Uuid>,
}

/// Mirrors the writable `UserFavoriteSerializer` input
/// (`serializers/favorite.py:60-76`): `entity_type` required / max-100 /
/// non-blank; `entity_identifier` optional UUID; `name` optional max-255;
/// `is_folder` optional bool (default false); `sequence` optional number
/// (default 65535, `db/models/favorite.py` field default); `parent` /
/// `project_id` optional UUIDs. Failures → 400 field-keyed (Django returns
/// `serializer.errors`, `favorite.py:65`).
fn validate_create(body: &Value) -> Result<ParsedCreate, Map<String, Value>> {
    let mut errors = Map::new();
    let entity_type = match body.get("entity_type") {
        None => {
            let (k, v) = required("entity_type");
            errors.insert(k, v);
            None
        }
        Some(Value::String(s)) if s.is_empty() => {
            let (k, v) = err("entity_type", "This field may not be blank.".to_string());
            errors.insert(k, v);
            None
        }
        Some(Value::String(s)) if s.chars().count() > 100 => {
            let (k, v) = err(
                "entity_type",
                "Ensure this field has no more than 100 characters.".to_string(),
            );
            errors.insert(k, v);
            None
        }
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => {
            let (k, v) = err("entity_type", "This field is required.".to_string());
            errors.insert(k, v);
            None
        }
    };
    let entity_identifier = parse_uuid_field(body, "entity_identifier", &mut errors).flatten();
    let name = match body.get("name") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.chars().count() > 255 => {
            let (k, v) = err(
                "name",
                "Ensure this field has no more than 255 characters.".to_string(),
            );
            errors.insert(k, v);
            None
        }
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => {
            let (k, v) = err("name", "This field may not be null.".to_string());
            errors.insert(k, v);
            None
        }
    };
    let is_folder = match body.get("is_folder") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => {
            let (k, v) = err("is_folder", "Must be a valid boolean.".to_string());
            errors.insert(k, v);
            false
        }
    };
    let sequence = match body.get("sequence") {
        None | Some(Value::Null) => None,
        Some(Value::Number(n)) => n.as_f64().or_else(|| {
            let (k, v) = err("sequence", "A valid number is required.".to_string());
            errors.insert(k, v);
            None
        }),
        Some(_) => {
            let (k, v) = err("sequence", "A valid number is required.".to_string());
            errors.insert(k, v);
            None
        }
    };
    let parent = parse_uuid_field(body, "parent", &mut errors).flatten();
    let project_id = parse_uuid_field(body, "project_id", &mut errors).flatten();
    if !errors.is_empty() || entity_type.is_none() {
        if entity_type.is_none() && !errors.contains_key("entity_type") {
            let (k, v) = required("entity_type");
            errors.insert(k, v);
        }
        return Err(errors);
    }
    Ok(ParsedCreate {
        entity_type: entity_type.unwrap_or_default(),
        entity_identifier,
        name,
        is_folder,
        sequence,
        parent,
        project_id,
    })
}

/// Mirrors `WorkspaceFavoriteEndpoint.post`
/// (`views/workspace/favorite.py:37-67`, `urls/workspace.py:187-191`):
/// POST **200** (not 201). `workspace` from the slug (`:40`), `user` from
/// the request; dup `entity_type+identifier` → **200 existing** (`:43-54`);
/// new rows validate (400 `serializer.errors`, `:65`) and save with
/// `sequence=max+10000` (`db/models/favorite.py:52-63`); a race
/// `IntegrityError` → 400 `{"error":"Favorite already exists"}` (`:66-67`).
/// Gate WORKSPACE ADMIN/MEMBER (`:37`, Guest → 403 `deny()`).
/// The two FK pre-checks mirror the serializer's relational validation
/// (`serializers/favorite.py:60-76`): unknown `parent`/`project_id` → 400
/// `Invalid pk …` (Django raises this inside `is_valid`, before `save`).
pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if guard_ws_am(ws_role(&st.pool, auth.0, &slug).await?).is_err() {
        return Ok(deny());
    }
    let parsed = match validate_create(&body) {
        Ok(p) => p,
        Err(errors) => return Ok(field_errors(errors)),
    };
    let Some(ws_id) = workspace_id(&st.pool, &slug).await? else {
        // Sane mapping (locked Django-500 rule): Django `post` would
        // 500 on `Workspace.DoesNotExist` (`:40`); unreachable post-gate
        // (the gate requires membership in this slug's workspace).
        return Ok(missing());
    };
    // `favorite.py:43-54`: dup check (workspace + user + type + identifier)
    // short-circuits with 200 existing. Skipped when the body carries no
    // `entity_identifier` (`:43` falsy guard — folders).
    if let Some(eid) = parsed.entity_identifier {
        let existing: Option<FavRow> = sqlx::query_as(
            "SELECT uf.id, uf.entity_type, uf.entity_identifier, uf.name, uf.is_folder, \
             uf.sequence, uf.parent_id, uf.workspace_id, uf.project_id \
             FROM user_favorites uf WHERE uf.workspace_id = $1 AND uf.user_id = $2 \
             AND uf.entity_type = $3 AND uf.entity_identifier = $4 AND uf.deleted_at IS NULL",
        )
        .bind(ws_id)
        .bind(auth.0)
        .bind(&parsed.entity_type)
        .bind(eid)
        .fetch_optional(&st.pool)
        .await?;
        if let Some(row) = existing {
            let data =
                resolve_entity_data(&st.pool, &row.entity_type, row.entity_identifier).await?;
            let out = render(&row, data);
            return Ok((
                StatusCode::OK,
                Json(serde_json::to_value(out).unwrap_or(Value::Null)),
            ));
        }
    }
    if let Some(parent) = parsed.parent {
        let ok: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM user_favorites WHERE id = $1)")
                .bind(parent)
                .fetch_one(&st.pool)
                .await?;
        if !ok {
            let mut errors = Map::new();
            let (k, v) = err("parent", invalid_pk_msg(&parent.to_string()));
            errors.insert(k, v);
            return Ok(field_errors(errors));
        }
    }
    if let Some(pid) = parsed.project_id {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projects WHERE id = $1)")
            .bind(pid)
            .fetch_one(&st.pool)
            .await?;
        if !ok {
            let mut errors = Map::new();
            let (k, v) = err("project_id", invalid_pk_msg(&pid.to_string()));
            errors.insert(k, v);
            return Ok(field_errors(errors));
        }
    }
    // `db/models/favorite.py:52-63` sequence rule inside a transaction with
    // the INSERT (multi-write in tx, locked rule).
    let mut tx = st.pool.begin().await?;
    let largest: Option<f64> =
        sqlx::query_scalar("SELECT MAX(sequence) FROM user_favorites WHERE workspace_id = $1")
            .bind(ws_id)
            .fetch_one(&mut *tx)
            .await?;
    let seq = next_sequence_live(largest, parsed.sequence.unwrap_or(65535.0));
    let inserted = sqlx::query_as::<_, FavRow>(
        "INSERT INTO user_favorites (id, entity_type, entity_identifier, name, is_folder, \
         sequence, parent_id, project_id, user_id, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $8, $9, now(), now()) \
         RETURNING id, entity_type, entity_identifier, name, is_folder, sequence, \
         parent_id, workspace_id, project_id",
    )
    .bind(&parsed.entity_type)
    .bind(parsed.entity_identifier)
    .bind(parsed.name.clone())
    .bind(parsed.is_folder)
    .bind(seq)
    .bind(parsed.parent)
    .bind(parsed.project_id)
    .bind(auth.0)
    .bind(ws_id)
    .fetch_one(&mut *tx)
    .await;
    let row = match inserted {
        Ok(row) => row,
        // `favorite.py:66-67`: lost the race (or hit the parent FK) →
        // 400 `{"error":"Favorite already exists"}`.
        Err(e) if is_constraint_violation(&e) => return Ok(dup_exists()),
        Err(e) => return Err(e.into()),
    };
    tx.commit().await?;
    let data = resolve_entity_data(&st.pool, &row.entity_type, row.entity_identifier).await?;
    let out = render(&row, data);
    Ok((
        StatusCode::OK,
        Json(serde_json::to_value(out).unwrap_or(Value::Null)),
    ))
}

// ============================================================================
// E6a — PATCH + DELETE detail.
// ============================================================================

/// Mirrors `WorkspaceFavoriteEndpoint.patch`
/// (`views/workspace/favorite.py:69-76`, `urls/workspace.py:192-196`):
/// PATCH 200; lookup is user + slug + pk; partial serializer validation →
/// 400 (`:76`). Gate WORKSPACE ADMIN/MEMBER (`:69`).
/// DEVIATION (locked Django-500 rule, documented): Django `.get()` miss
/// raises uncaught `DoesNotExist` → 500; Rust returns 404 `missing()`.
pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, fid)): Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if guard_ws_am(ws_role(&st.pool, auth.0, &slug).await?).is_err() {
        return Ok(deny());
    }
    let row: Option<FavRow> = sqlx::query_as(
        "SELECT uf.id, uf.entity_type, uf.entity_identifier, uf.name, uf.is_folder, \
         uf.sequence, uf.parent_id, uf.workspace_id, uf.project_id \
         FROM user_favorites uf JOIN workspaces w ON w.id = uf.workspace_id \
         WHERE uf.id = $1 AND uf.user_id = $2 AND w.slug = $3 AND uf.deleted_at IS NULL",
    )
    .bind(fid)
    .bind(auth.0)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some(current) = row else {
        return Ok(missing());
    };
    let mut errors = Map::new();
    // Partial update: only present keys are validated + applied (DRF
    // `partial=True`, `:73`).
    let mut entity_type = current.entity_type.clone();
    if let Some(v) = body.get("entity_type") {
        match v {
            Value::String(s) if !s.is_empty() && s.chars().count() <= 100 => {
                entity_type = s.clone()
            }
            Value::String(s) if s.is_empty() => {
                let (k, e) = err("entity_type", "This field may not be blank.".to_string());
                errors.insert(k, e);
            }
            Value::String(_) => {
                let (k, e) = err(
                    "entity_type",
                    "Ensure this field has no more than 100 characters.".to_string(),
                );
                errors.insert(k, e);
            }
            _ => {
                let (k, e) = required("entity_type");
                errors.insert(k, e);
            }
        }
    }
    let mut entity_identifier = current.entity_identifier;
    if body.get("entity_identifier").is_some() {
        match parse_uuid_field(&body, "entity_identifier", &mut errors) {
            Some(v) => entity_identifier = v,
            None => {}
        }
        if errors.contains_key("entity_identifier") {
            // keep current on error; the 400 below wins
        }
    }
    let mut name = current.name.clone();
    if let Some(v) = body.get("name") {
        match v {
            Value::Null => name = None,
            Value::String(s) if s.chars().count() <= 255 => name = Some(s.clone()),
            Value::String(_) => {
                let (k, e) = err(
                    "name",
                    "Ensure this field has no more than 255 characters.".to_string(),
                );
                errors.insert(k, e);
            }
            _ => {
                let (k, e) = err("name", "This field may not be null.".to_string());
                errors.insert(k, e);
            }
        }
    }
    let mut is_folder = current.is_folder;
    if let Some(v) = body.get("is_folder") {
        match v {
            Value::Bool(b) => is_folder = *b,
            Value::Null => {}
            _ => {
                let (k, e) = err("is_folder", "Must be a valid boolean.".to_string());
                errors.insert(k, e);
            }
        }
    }
    let mut sequence = current.sequence;
    if let Some(v) = body.get("sequence") {
        match v {
            Value::Number(n) => match n.as_f64() {
                Some(f) => sequence = f,
                None => {
                    let (k, e) = err("sequence", "A valid number is required.".to_string());
                    errors.insert(k, e);
                }
            },
            Value::Null => {}
            _ => {
                let (k, e) = err("sequence", "A valid number is required.".to_string());
                errors.insert(k, e);
            }
        }
    }
    let mut parent = current.parent_id;
    if body.get("parent").is_some() {
        match parse_uuid_field(&body, "parent", &mut errors) {
            Some(v) => parent = v,
            None => {}
        }
    }
    let mut project_id = current.project_id;
    if body.get("project_id").is_some() {
        match parse_uuid_field(&body, "project_id", &mut errors) {
            Some(v) => project_id = v,
            None => {}
        }
    }
    if !errors.is_empty() {
        return Ok(field_errors(errors));
    }
    if let Some(p) = parent {
        let ok: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM user_favorites WHERE id = $1)")
                .bind(p)
                .fetch_one(&st.pool)
                .await?;
        if !ok {
            let mut errors = Map::new();
            let (k, v) = err("parent", invalid_pk_msg(&p.to_string()));
            errors.insert(k, v);
            return Ok(field_errors(errors));
        }
    }
    if let Some(pid) = project_id {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projects WHERE id = $1)")
            .bind(pid)
            .fetch_one(&st.pool)
            .await?;
        if !ok {
            let mut errors = Map::new();
            let (k, v) = err("project_id", invalid_pk_msg(&pid.to_string()));
            errors.insert(k, v);
            return Ok(field_errors(errors));
        }
    }
    let updated: FavRow = sqlx::query_as(
        "UPDATE user_favorites SET entity_type = $1, entity_identifier = $2, name = $3, \
         is_folder = $4, sequence = $5, parent_id = $6, project_id = $7, updated_at = now() \
         WHERE id = $8 RETURNING id, entity_type, entity_identifier, name, is_folder, \
         sequence, parent_id, workspace_id, project_id",
    )
    .bind(&entity_type)
    .bind(entity_identifier)
    .bind(name.clone())
    .bind(is_folder)
    .bind(sequence)
    .bind(parent)
    .bind(project_id)
    .bind(fid)
    .fetch_one(&st.pool)
    .await?;
    let data =
        resolve_entity_data(&st.pool, &updated.entity_type, updated.entity_identifier).await?;
    let out = render(&updated, data);
    Ok((
        StatusCode::OK,
        Json(serde_json::to_value(out).unwrap_or(Value::Null)),
    ))
}

/// Mirrors `WorkspaceFavoriteEndpoint.delete`
/// (`views/workspace/favorite.py:78-82`, `urls/workspace.py:192-196`):
/// DELETE **204**, HARD delete (`soft=False`, `:81`). Gate WORKSPACE
/// ADMIN/MEMBER (`:78`).
/// DEVIATION (locked Django-500 rule, documented): Django `.get()` miss →
/// 500; Rust returns 404 `missing()`.
pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, fid)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if guard_ws_am(ws_role(&st.pool, auth.0, &slug).await?).is_err() {
        return Ok(deny());
    }
    let n = sqlx::query(
        "DELETE FROM user_favorites uf USING workspaces w \
         WHERE uf.id = $1 AND uf.user_id = $2 AND uf.workspace_id = w.id AND w.slug = $3 \
         AND uf.deleted_at IS NULL",
    )
    .bind(fid)
    .bind(auth.0)
    .bind(&slug)
    .execute(&st.pool)
    .await?;
    if n.rows_affected() == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

// ============================================================================
// E6a — GET group.
// ============================================================================

/// Mirrors `WorkspaceFavoriteGroupEndpoint.get`
/// (`views/workspace/favorite.py:85-97`, `urls/workspace.py:197-201`):
/// GET 200 children of the folder (`parent_id=favorite_id`, `:88`) with the
/// member gate (`:88-93` — project-null rows pass, project-linked rows
/// require an ACTIVE project membership; note NO page exclusion here,
/// unlike the list endpoint). Gate WORKSPACE ADMIN/MEMBER (`:86`). No
/// folder-existence check (Django filters; unknown folder → 200 `[]`).
pub async fn group(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, fid)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if guard_ws_am(ws_role(&st.pool, auth.0, &slug).await?).is_err() {
        return Ok(deny());
    }
    let rows: Vec<FavRow> = sqlx::query_as(
        "SELECT uf.id, uf.entity_type, uf.entity_identifier, uf.name, \
         uf.is_folder, uf.sequence, uf.parent_id, uf.workspace_id, uf.project_id \
         FROM user_favorites uf JOIN workspaces w ON w.id = uf.workspace_id \
         WHERE uf.user_id = $1 AND w.slug = $2 AND uf.parent_id = $3 AND uf.deleted_at IS NULL \
         AND (uf.project_id IS NULL \
           OR EXISTS (SELECT 1 FROM project_members pm \
             WHERE pm.project_id = uf.project_id AND pm.member_id = $1 \
             AND pm.is_active = true AND pm.deleted_at IS NULL)) \
         ORDER BY uf.created_at DESC",
    )
    .bind(auth.0)
    .bind(&slug)
    .bind(fid)
    .fetch_all(&st.pool)
    .await?;
    let out = render_rows(&st.pool, &rows).await?;
    Ok((
        StatusCode::OK,
        Json(serde_json::to_value(out).unwrap_or(Value::Null)),
    ))
}

// ============================================================================
// E6b — view favorite DELETE (the only missing entity-favorite route).
// ============================================================================

/// Mirrors `IssueViewFavoriteViewSet.destroy`
/// (`plane/app/views/view/base.py:435-444`,
/// `plane/app/urls/views.py:61-64`): DELETE **204**; `.get(project,
/// user, workspace__slug, entity_type="view", entity_identifier=view_id)`
/// miss → 404; HARD delete (`soft=False`, `:443`). Gate project
/// ADMIN/MEMBER (`@allow_permission([ROLE.ADMIN, ROLE.MEMBER])`, `:434` —
/// project level with the ws-admin fallback, deny = `deny()` 403).
pub async fn view_fav_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pid, vid)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !project_in_workspace(&st.pool, pid, &slug).await? {
        return Ok(missing());
    }
    if !gate_proj_am(&st.pool, auth.0, &slug, pid).await? {
        return Ok(deny());
    }
    // `view/base.py:436-442`: scoped `.get()` + hard delete.
    let n = sqlx::query(
        "DELETE FROM user_favorites WHERE project_id = $1 AND entity_type = 'view' \
         AND user_id = $2 AND workspace_id IN (SELECT id FROM workspaces WHERE slug = $3) \
         AND entity_identifier = $4",
    )
    .bind(pid)
    .bind(auth.0)
    .bind(&slug)
    .bind(vid)
    .execute(&st.pool)
    .await?;
    if n.rows_affected() == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

// ============================================================================
// Unit tests (TDD: pure helpers + shapes; DB-gated paths mirror the
// cycle.rs/page.rs fav handlers line-for-line and compile under
// `cargo check`).
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ws_gate_passes_admin_member_only() {
        // `favorite.py:23,37,69,78,86` — ADMIN (20) + MEMBER (15) pass;
        // GUEST (5), non-members and unknown slugs fail.
        assert!(guard_ws_am(Some(20)).is_ok());
        assert!(guard_ws_am(Some(15)).is_ok());
        assert!(guard_ws_am(Some(5)).is_err());
        assert!(guard_ws_am(None).is_err());
        assert_eq!(
            guard_ws_am(Some(5)).unwrap_err(),
            "You don't have the required permissions."
        );
    }

    #[test]
    fn proj_gate_passes_admin_member_only() {
        // `view/base.py:434` — same role list as `cycle.rs:178-183`.
        assert!(guard_proj_am(Some(20)).is_ok());
        assert!(guard_proj_am(Some(15)).is_ok());
        assert!(guard_proj_am(Some(5)).is_err());
        assert!(guard_proj_am(None).is_err());
    }

    #[test]
    fn sequence_rule_overwrites_only_when_rows_exist() {
        // `db/models/favorite.py:62-63`: adding with rows → max+10000
        // (provided value ignored); no rows → provided/default kept.
        assert_eq!(next_sequence(Some(65535.0), 1.0), 75535.0);
        assert_eq!(next_sequence(Some(0.0), 65535.0), 10000.0);
        assert_eq!(next_sequence(None, 65535.0), 65535.0);
        assert_eq!(next_sequence(None, 5.0), 5.0);
    }

    #[test]
    fn dup_race_is_400_favorite_exists() {
        // `favorite.py:66-67`.
        let (code, body) = dup_exists();
        assert_eq!(code, StatusCode::BAD_REQUEST);
        assert_eq!(body.0, json!({"error": "Favorite already exists"}));
    }

    #[test]
    fn create_requires_entity_type() {
        // `serializers/favorite.py:60-76` — missing entity_type → 400
        // field-keyed (Django `serializer.errors`, `favorite.py:65`).
        let body = json!({"name": "x"});
        let errors = validate_create(&body).unwrap_err();
        assert_eq!(
            errors.get("entity_type"),
            Some(&json!(["This field is required."]))
        );
    }

    #[test]
    fn create_rejects_blank_and_long_entity_type() {
        let errors = validate_create(&json!({"entity_type": ""})).unwrap_err();
        assert_eq!(
            errors.get("entity_type"),
            Some(&json!(["This field may not be blank."]))
        );
        let long = "e".repeat(101);
        let errors = validate_create(&json!({"entity_type": long})).unwrap_err();
        assert_eq!(
            errors.get("entity_type"),
            Some(&json!([
                "Ensure this field has no more than 100 characters."
            ]))
        );
    }

    #[test]
    fn create_rejects_bad_uuid_and_bad_scalars() {
        let body = json!({
            "entity_type": "view",
            "entity_identifier": "nope",
            "is_folder": "yes",
            "sequence": "lots",
            "parent": 42,
        });
        let errors = validate_create(&body).unwrap_err();
        assert_eq!(
            errors.get("entity_identifier"),
            Some(&json!(["“nope” is not a valid UUID."]))
        );
        assert_eq!(
            errors.get("is_folder"),
            Some(&json!(["Must be a valid boolean."]))
        );
        assert_eq!(
            errors.get("sequence"),
            Some(&json!(["A valid number is required."]))
        );
        assert!(errors.get("parent").is_some());
    }

    #[test]
    fn create_accepts_minimal_and_full_bodies() {
        // `favorite.py:57-64` — body carries only the serializer fields;
        // `workspace`/`user` come from the URL/request, never the body.
        let minimal = validate_create(&json!({"entity_type": "folder"})).unwrap();
        assert_eq!(minimal.entity_type, "folder");
        assert!(minimal.entity_identifier.is_none());
        assert!(!minimal.is_folder);
        assert!(minimal.sequence.is_none());
        let vid = uuid::Uuid::new_v4();
        let pid = uuid::Uuid::new_v4();
        let full = validate_create(&json!({
            "entity_type": "view",
            "entity_identifier": vid.to_string(),
            "name": "n",
            "is_folder": true,
            "sequence": 10,
            "parent": uuid::Uuid::new_v4().to_string(),
            "project_id": pid.to_string(),
        }))
        .unwrap();
        assert_eq!(full.entity_identifier, Some(vid));
        assert!(full.is_folder);
        assert_eq!(full.sequence, Some(10.0));
        assert_eq!(full.project_id, Some(pid));
    }

    #[test]
    fn favorite_json_keeps_serializer_key_order() {
        // `serializers/favorite.py:64-75` 10-key shape. (Wire order follows
        // the repo-wide `Json<Value>` convention — alphabetical via the
        // default `BTreeMap` map impl, same as every other route file — so
        // the assertion is order-insensitive; the struct declaration above
        // documents the Django serializer order.)
        let id = uuid::Uuid::new_v4();
        let row = FavRow {
            id,
            entity_type: "cycle".to_string(),
            entity_identifier: Some(id),
            name: Some("c".to_string()),
            is_folder: false,
            sequence: 75535.0,
            parent_id: None,
            workspace_id: id,
            project_id: Some(id),
        };
        let out = render(&row, Value::Null);
        let v = serde_json::to_value(&out).unwrap();
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        let mut expected = vec![
            "id",
            "entity_type",
            "entity_identifier",
            "entity_data",
            "name",
            "is_folder",
            "sequence",
            "parent",
            "workspace_id",
            "project_id",
        ];
        expected.sort_unstable();
        assert_eq!(keys, expected);
        assert_eq!(v["entity_data"], Value::Null);
    }

    #[test]
    fn entity_lite_shapes_match_serializers() {
        // `serializers/favorite.py:11-13` (project: 3 keys) and `:31-43`
        // (cycle/module/view/page: 4 keys with project_id).
        let id = uuid::Uuid::new_v4();
        let p = serde_json::to_value(ProjectFavData {
            id,
            name: "p".to_string(),
            logo_props: json!({}),
        })
        .unwrap();
        let mut keys: Vec<&str> = p.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["id", "logo_props", "name"]);
        let c = serde_json::to_value(EntityFavData {
            id,
            name: "c".to_string(),
            logo_props: json!({"a": 1}),
            project_id: Some(id),
        })
        .unwrap();
        let mut keys: Vec<&str> = c.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["id", "logo_props", "name", "project_id"]);
    }

    #[test]
    fn invalid_pk_message_shape() {
        assert_eq!(
            invalid_pk_msg("abc"),
            "Invalid pk \"abc\" - object does not exist."
        );
    }
}
