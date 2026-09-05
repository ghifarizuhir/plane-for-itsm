use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::routes::project::{deny, is_integrity_error, missing, FORBIDDEN_MSG};
use crate::{middleware::auth::AuthUser, state::AppState};

use super::history::{actor_detail_json, comment_reaction_json, CommentReactionRow};
use super::issue_common::{fetch_project_member_role, is_workspace_admin, project_gate_allows};

/// Quoted from `plane/app/views/base.py:80-84` (Django `handle_exception` maps
/// EVERY `IntegrityError` → 400). `IssueReactionViewSet.create`
/// (`plane/app/views/issue/reaction.py:46-62`) has NO explicit dup catch, so a
/// duplicate `(issue, actor, reaction)` unique violation
/// (`db/models/issue.py:609-617`) surfaces with exactly this body.
pub(crate) const ISSUE_DUP_MSG: &str = "The payload is not valid";
/// Quoted from `plane/app/views/issue/comment.py:208`.
/// `CommentReactionViewSet.create` (`comment.py:184-210`) catches
/// `IntegrityError` explicitly, so a duplicate `(comment, actor, reaction)`
/// unique violation (`db/models/issue.py:637-644`) surfaces with this body
/// instead of the generic one above.
pub(crate) const COMMENT_DUP_MSG: &str = "Reaction already exists for the user";

/// PROJECT-level role check shared by all six handlers: mirrors
/// `@allow_permission([ROLE.ADMIN, ROLE.MEMBER, ROLE.GUEST])`
/// (`reaction.py:45,64`, `comment.py:183,212`, decorator default
/// `level="PROJECT"`, `permissions/base.py:17`) — roles 20/15/5 pass; anything
/// else (incl. non-member) falls to the workspace-ADMIN fallback applied by
/// the caller via the shared `project_gate_allows` (same shape as D5/D6/D7).
pub(crate) fn guard_reactions(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) | Some(5) => Ok(()),
        _ => Err(FORBIDDEN_MSG.to_string()),
    }
}

/// Pure encoding of the DELETE actor scope: both `destroy` methods filter
/// `actor=request.user` (`reaction.py:66-72`, `comment.py:214-220`), so a row
/// is deletable iff its actor is the caller. The handlers enforce this in SQL
/// (`AND r.actor_id = $user`); this fn pins the semantics for tests.
#[allow(dead_code)] // test-only pin of the SQL scope (D7 `ARCHIVED_DETAIL_KEYS` precedent).
pub(crate) fn reaction_scope_ok(row_actor_id: uuid::Uuid, user_id: uuid::Uuid) -> bool {
    row_actor_id == user_id
}

/// DRF required/blank validation for the `reaction` field: missing →
/// `{"reaction": ["This field is required."]}`, blank (post-trim, mirroring
/// DRF `trim_whitespace`) → `{"reaction": ["This field may not be blank."]}`.
pub(crate) fn validate_reaction(body: &ReactionBody) -> Result<String, Value> {
    match &body.reaction {
        None => Err(json!({"reaction": ["This field is required."]})),
        Some(r) if r.trim().is_empty() => {
            Err(json!({"reaction": ["This field may not be blank."]}))
        }
        Some(r) => Ok(r.clone()),
    }
}

/// POST body for both create handlers: mirrors the writable `reaction` field
/// of `IssueReactionSerializer` / `CommentReactionSerializer` (everything
/// else — issue/comment, project, actor, workspace — comes from the URL and
/// the request user, `reaction.py:49`, `comment.py:188-192`).
#[derive(Debug, Clone, Deserialize)]
pub struct ReactionBody {
    pub reaction: Option<String>,
}

/// Shared PROJECT gate for all six handlers: the outer
/// `@allow_permission([ADMIN, MEMBER, GUEST])` check with the standard
/// workspace-ADMIN fallback (`permissions/base.py:53-78`) via
/// `project_gate_allows` — exactly the D6 `versions_gate` shape.
async fn reactions_gate(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    slug: &str,
    project_id: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let member_role = fetch_project_member_role(pool, user_id, slug, project_id).await?;
    let ws_admin = is_workspace_admin(pool, user_id, slug).await?;
    Ok(project_gate_allows(
        guard_reactions(member_role).is_ok(),
        member_role.is_some(),
        ws_admin,
    ))
}

/// Maps a Postgres error to `true` iff it is an integrity-constraint violation
/// (SQLSTATE class `23`: `23505` unique for dups, `23503` FK for unknown
/// issue/comment/project ids). Reuses the shared `project::is_integrity_error`
/// probe — Django's blanket `IntegrityError` mapping (`views/base.py:80-84`)
/// covers both classes identically.
fn is_integrity_err(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .map(|c| is_integrity_error(c.as_ref()))
        .unwrap_or(false)
}

fn opt_id(id: &Option<uuid::Uuid>) -> Value {
    id.map(|u| json!(u)).unwrap_or(Value::Null)
}

/// One `IssueReactionSerializer` row (`serializers/issue.py:649-655`:
/// `fields = "__all__"` → id, created_at, updated_at, created_by, updated_by,
/// deleted_at, project, workspace, actor, issue, reaction — plus the
/// `actor_detail` UserLite object). Live columns verified in
/// `apps/api-rs/migrations/0001_initial.sql` (`issue_reactions` table). Field
/// names match the SELECT aliases below.
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct IssueReactionRow {
    pub(crate) id: uuid::Uuid,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) updated_at: chrono::DateTime<chrono::Utc>,
    pub(crate) created_by_id: Option<uuid::Uuid>,
    pub(crate) updated_by_id: Option<uuid::Uuid>,
    pub(crate) deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) project_id: uuid::Uuid,
    pub(crate) workspace_id: uuid::Uuid,
    pub(crate) actor_id: Option<uuid::Uuid>,
    pub(crate) issue_id: uuid::Uuid,
    pub(crate) reaction: String,
    pub(crate) a_first_name: Option<String>,
    pub(crate) a_last_name: Option<String>,
    pub(crate) a_avatar: Option<String>,
    pub(crate) a_avatar_asset_id: Option<uuid::Uuid>,
    pub(crate) a_avatar_entity_type: Option<String>,
    pub(crate) a_is_bot: Option<bool>,
    pub(crate) a_display_name: Option<String>,
}

/// Serializes one `IssueReactionRow` like `IssueReactionSerializer`
/// (`serializers/issue.py:649-655`): FKs render as id strings (DRF default PK
/// representation); `actor_detail` is the `UserLiteSerializer` object
/// (`serializers/user.py:141-153`), reusing history's `actor_detail_json`
/// (same shape as D1 `subscriber_member_json`).
pub(crate) fn issue_reaction_json(row: &IssueReactionRow) -> Value {
    json!({
        "id": row.id,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "created_by": opt_id(&row.created_by_id),
        "updated_by": opt_id(&row.updated_by_id),
        "deleted_at": row.deleted_at,
        "project": row.project_id,
        "workspace": row.workspace_id,
        "actor": opt_id(&row.actor_id),
        "issue": row.issue_id,
        "reaction": row.reaction,
        "actor_detail": actor_detail_json(
            row.actor_id, &row.a_first_name, &row.a_last_name, &row.a_avatar,
            row.a_avatar_asset_id, row.a_avatar_entity_type.as_deref(),
            &row.a_is_bot, &row.a_display_name,
        ),
    })
}

/// Shared SELECT for issue reactions: the row columns plus the actor UserLite
/// columns (`users` + `file_assets` for `avatar_url`, same join as D1/D2).
/// Scoped by the caller (list scope for GET, row id for the POST re-read).
const ISSUE_REACTION_SELECT: &str = "r.id, r.created_at, r.updated_at, \
    r.created_by_id, r.updated_by_id, r.deleted_at, r.project_id, r.workspace_id, \
    r.actor_id, r.issue_id, r.reaction, \
    u.first_name AS a_first_name, u.last_name AS a_last_name, \
    u.avatar AS a_avatar, u.avatar_asset_id AS a_avatar_asset_id, \
    fa.entity_type AS a_avatar_entity_type, u.is_bot AS a_is_bot, \
    u.display_name AS a_display_name \
    FROM issue_reactions r \
    LEFT JOIN users u ON u.id = r.actor_id \
    LEFT JOIN file_assets fa ON fa.id = u.avatar_asset_id";

/// Shared SELECT for comment reactions: the `CommentReactionSerializer` row
/// columns (`serializers/issue.py:666-685`) plus the actor display name.
/// Maps into history's `CommentReactionRow` (identical shape — reused, not
/// forked) and serializes via history's `comment_reaction_json`.
const COMMENT_REACTION_SELECT: &str = "r.id, r.actor_id, r.comment_id, r.reaction, \
    u.display_name AS actor_display_name, r.deleted_at, r.workspace_id, r.project_id, \
    r.created_at, r.updated_at, r.created_by_id, r.updated_by_id \
    FROM comment_reactions r \
    LEFT JOIN users u ON u.id = r.actor_id";

/// GET `.../issues/:issue_id/reactions/` — parity with Django's default
/// `list` over `IssueReactionViewSet.get_queryset` (`reaction.py:29-43`,
/// `urls/issue.py:191-195`): NO custom `list` exists, so the queryset IS the
/// contract — slug + project + issue scope, active project membership of the
/// caller, non-archived project, live rows only, `ORDER BY created_at DESC`.
/// Gate AMG + ws-admin fallback (see `reactions_gate`).
pub async fn issue_reactions_list(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
    )>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !reactions_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let rows: Vec<IssueReactionRow> = sqlx::query_as(&format!(
        "SELECT {ISSUE_REACTION_SELECT} \
        JOIN workspaces w ON w.id = r.workspace_id \
        JOIN projects p ON p.id = r.project_id \
        WHERE w.slug = $1 AND r.project_id = $2 AND r.issue_id = $3 \
        AND r.deleted_at IS NULL \
        AND p.archived_at IS NULL AND p.deleted_at IS NULL \
        AND EXISTS(SELECT 1 FROM project_members pm \
          WHERE pm.project_id = r.project_id AND pm.member_id = $4 \
          AND pm.is_active = true AND pm.deleted_at IS NULL) \
        ORDER BY r.created_at DESC"
    ))
    .bind(&slug)
    .bind(project_id)
    .bind(issue_id)
    .bind(auth.0)
    .fetch_all(&st.pool)
    .await?;
    let out: Vec<Value> = rows.iter().map(issue_reaction_json).collect();
    Ok((StatusCode::OK, Json(Value::Array(out))))
}

/// POST `.../issues/:issue_id/reactions/` — parity with
/// `IssueReactionViewSet.create` (`reaction.py:46-62`,
/// `urls/issue.py:191-195`).
///
/// - Gate AMG + ws-admin fallback.
/// - `workspace` derives from the project (`ProjectBaseModel.save`,
///   `project.py:187-189`); `created_by`/`actor` = the requester
///   (`BaseModel.save`, `reaction.py:49`); `updated_by` stays NULL → **201**
///   `IssueReactionSerializer` + `actor_detail`.
/// - Dup `(issue, actor, reaction)` → unique violation → 400
///   `{"error": "The payload is not valid"}` via the base handler mapping
///   (NO explicit catch in Django — mirrored by the class-`23` probe).
///
/// Deviations: none on the wire — datetimes RFC3339 (batch convention);
/// Celery `issue_activity.delay` skipped (batch-wide precedent).
pub async fn issue_reaction_create(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
    )>,
    Json(body): Json<ReactionBody>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !reactions_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let reaction = match validate_reaction(&body) {
        Ok(r) => r,
        Err(v) => return Ok((StatusCode::BAD_REQUEST, Json(v))),
    };
    let new_id = match sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO issue_reactions (id, reaction, actor_id, issue_id, project_id, workspace_id, created_by_id, created_at, updated_at) \
        SELECT gen_random_uuid(), $1, $2, $3, $4, p.workspace_id, $2, now(), now() FROM projects p WHERE p.id = $4 \
        RETURNING id",
    )
    .bind(&reaction)
    .bind(auth.0)
    .bind(issue_id)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await
    {
        Ok(v) => v,
        Err(e) if is_integrity_err(&e) => {
            // NO explicit catch in Django (`reaction.py:46-62`) — the dup
            // `IntegrityError` falls through to the base handler
            // (`views/base.py:80-84`).
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": ISSUE_DUP_MSG})),
            ));
        }
        Err(e) => return Err(e.into()),
    };
    // Unreachable when the gate passes (the gate's slug-scoped membership
    // lookup already proved the project exists), but Django would 500 on a
    // missing project while Rust returns the standard 404 instead (D1/D5
    // precedent).
    let Some(new_id) = new_id else {
        return Ok(missing());
    };
    let row: IssueReactionRow =
        sqlx::query_as(&format!("SELECT {ISSUE_REACTION_SELECT} WHERE r.id = $1"))
            .bind(new_id)
            .fetch_one(&st.pool)
            .await?;
    Ok((StatusCode::CREATED, Json(issue_reaction_json(&row))))
}

/// DELETE `.../issues/:issue_id/reactions/:reaction_code/` — parity with
/// `IssueReactionViewSet.destroy` (`reaction.py:64-85`,
/// `urls/issue.py:196-200`).
///
/// - Gate AMG + ws-admin fallback.
/// - `:reaction_code` is `str` (e.g. `heart`, `urls/issue.py:198`
///   `<str:reaction_code>`), scoped `(slug, project, issue, reaction, actor=user)`
///   (`reaction.py:66-72`); miss → 404 `missing()` (Django `.get()` →
///   `DoesNotExist` → `views/base.py:92-96`); success is a soft-delete
///   (Django default-manager `.delete()`) → **204** empty.
///
/// Deviations: none on the wire; Celery skipped.
pub async fn issue_reaction_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id, reaction_code)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
        String,
    )>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !reactions_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let n = sqlx::query(
        "UPDATE issue_reactions r SET deleted_at = now() FROM workspaces w \
        WHERE r.workspace_id = w.id AND w.slug = $1 AND r.project_id = $2 \
        AND r.issue_id = $3 AND r.reaction = $4 AND r.actor_id = $5 AND r.deleted_at IS NULL",
    )
    .bind(&slug)
    .bind(project_id)
    .bind(issue_id)
    .bind(&reaction_code)
    .bind(auth.0)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

/// GET `.../comments/:comment_id/reactions/` — parity with Django's default
/// `list` over `CommentReactionViewSet.get_queryset` (`comment.py:167-181`,
/// `urls/issue.py:203-207`): same shape as the issue twin, scoped to the
/// comment instead of the issue. Gate AMG + ws-admin fallback.
pub async fn comment_reactions_list(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, comment_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
    )>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !reactions_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let rows: Vec<CommentReactionRow> = sqlx::query_as(&format!(
        "SELECT {COMMENT_REACTION_SELECT} \
        JOIN workspaces w ON w.id = r.workspace_id \
        JOIN projects p ON p.id = r.project_id \
        WHERE w.slug = $1 AND r.project_id = $2 AND r.comment_id = $3 \
        AND r.deleted_at IS NULL \
        AND p.archived_at IS NULL AND p.deleted_at IS NULL \
        AND EXISTS(SELECT 1 FROM project_members pm \
          WHERE pm.project_id = r.project_id AND pm.member_id = $4 \
          AND pm.is_active = true AND pm.deleted_at IS NULL) \
        ORDER BY r.created_at DESC"
    ))
    .bind(&slug)
    .bind(project_id)
    .bind(comment_id)
    .bind(auth.0)
    .fetch_all(&st.pool)
    .await?;
    let out: Vec<Value> = rows.iter().map(comment_reaction_json).collect();
    Ok((StatusCode::OK, Json(Value::Array(out))))
}

/// POST `.../comments/:comment_id/reactions/` — parity with
/// `CommentReactionViewSet.create` (`comment.py:184-210`,
/// `urls/issue.py:203-207`).
///
/// - Gate AMG + ws-admin fallback; same `workspace`-from-project /
///   `created_by`-and-`actor`=requester derivation as the issue twin
///   (`comment.py:188-192`) → **201** `CommentReactionSerializer`
///   (`serializers/issue.py:666-685`, 12 keys incl. `display_name`).
/// - Dup `(comment, actor, reaction)` → `IntegrityError` → 400
///   `{"error": "Reaction already exists for the user"}`
///   (`comment.py:206-210`, explicit catch — differs from the issue twin).
///   The catch wraps the whole save, so any class-`23` violation (incl. an
///   FK miss on an unknown comment id) maps to the same body — mirrored
///   literally.
///
/// Deviations: none on the wire; Celery skipped.
pub async fn comment_reaction_create(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, comment_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
    )>,
    Json(body): Json<ReactionBody>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !reactions_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let reaction = match validate_reaction(&body) {
        Ok(r) => r,
        Err(v) => return Ok((StatusCode::BAD_REQUEST, Json(v))),
    };
    let res = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO comment_reactions (id, reaction, actor_id, comment_id, project_id, workspace_id, created_by_id, created_at, updated_at) \
        SELECT gen_random_uuid(), $1, $2, $3, $4, p.workspace_id, $2, now(), now() FROM projects p WHERE p.id = $4 \
        RETURNING id",
    )
    .bind(&reaction)
    .bind(auth.0)
    .bind(comment_id)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await;
    let new_id = match res {
        Ok(v) => v,
        Err(e) if is_integrity_err(&e) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": COMMENT_DUP_MSG})),
            ));
        }
        Err(e) => return Err(e.into()),
    };
    let Some(new_id) = new_id else {
        return Ok(missing());
    };
    let row: CommentReactionRow =
        sqlx::query_as(&format!("SELECT {COMMENT_REACTION_SELECT} WHERE r.id = $1"))
            .bind(new_id)
            .fetch_one(&st.pool)
            .await?;
    Ok((StatusCode::CREATED, Json(comment_reaction_json(&row))))
}

/// DELETE `.../comments/:comment_id/reactions/:reaction_code/` — parity with
/// `CommentReactionViewSet.destroy` (`comment.py:212-239`,
/// `urls/issue.py:208-212`): same semantics as the issue twin, scoped
/// `(slug, project, comment, reaction, actor=user)` (`comment.py:214-220`);
/// miss → 404 `missing()`; success → soft-delete → **204** empty.
/// Deviations: none on the wire; Celery skipped.
pub async fn comment_reaction_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, comment_id, reaction_code)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
        String,
    )>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !reactions_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let n = sqlx::query(
        "UPDATE comment_reactions r SET deleted_at = now() FROM workspaces w \
        WHERE r.workspace_id = w.id AND w.slug = $1 AND r.project_id = $2 \
        AND r.comment_id = $3 AND r.reaction = $4 AND r.actor_id = $5 AND r.deleted_at IS NULL",
    )
    .bind(&slug)
    .bind(project_id)
    .bind(comment_id)
    .bind(&reaction_code)
    .bind(auth.0)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

#[cfg(test)]
mod batch_d_d8_tests {
    use super::*;

    #[test]
    fn issue_dup_message_matches_django_base_handler() {
        // `IssueReactionViewSet.create` has NO explicit catch
        // (`reaction.py:46-62`); the dup `IntegrityError` falls through to
        // `handle_exception` (`views/base.py:80-84`).
        assert_eq!(ISSUE_DUP_MSG, "The payload is not valid");
    }

    #[test]
    fn comment_dup_message_matches_django_explicit_catch() {
        // Quoted from `plane/app/views/issue/comment.py:208`.
        assert_eq!(COMMENT_DUP_MSG, "Reaction already exists for the user");
    }

    #[test]
    fn reaction_scope_ok_pins_actor_scoping() {
        // Both `destroy` methods filter `actor=request.user`
        // (`reaction.py:66-72`, `comment.py:214-220`).
        let user = uuid::Uuid::nil();
        let other = uuid::Uuid::max();
        assert!(reaction_scope_ok(user, user));
        assert!(!reaction_scope_ok(other, user));
    }

    #[test]
    fn guard_reactions_is_admin_member_guest() {
        // Mirrors `@allow_permission([ADMIN, MEMBER, GUEST])`
        // (`reaction.py:45,64`, `comment.py:183,212`); anything else falls to
        // the workspace-ADMIN fallback via `project_gate_allows`.
        assert!(guard_reactions(Some(20)).is_ok());
        assert!(guard_reactions(Some(15)).is_ok());
        assert!(guard_reactions(Some(5)).is_ok());
        assert!(guard_reactions(Some(10)).is_err());
        assert_eq!(
            guard_reactions(None).unwrap_err(),
            "You don't have the required permissions."
        );
    }

    #[test]
    fn validate_reaction_mirrors_drf_required_and_blank() {
        assert_eq!(
            validate_reaction(&ReactionBody { reaction: None }).unwrap_err(),
            json!({"reaction": ["This field is required."]})
        );
        assert_eq!(
            validate_reaction(&ReactionBody {
                reaction: Some("   ".to_string())
            })
            .unwrap_err(),
            json!({"reaction": ["This field may not be blank."]})
        );
        assert_eq!(
            validate_reaction(&ReactionBody {
                reaction: Some("heart".to_string())
            })
            .unwrap(),
            "heart"
        );
    }

    #[test]
    fn issue_reaction_json_covers_serializer_keys_plus_actor_detail() {
        // Mirrors `IssueReactionSerializer` (`serializers/issue.py:649-655`,
        // `__all__` = 11 model columns) + `actor_detail` (7-key UserLite,
        // `serializers/user.py:141-153`).
        let row = IssueReactionRow {
            id: uuid::Uuid::nil(),
            created_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            updated_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            created_by_id: Some(uuid::Uuid::nil()),
            updated_by_id: None,
            deleted_at: None,
            project_id: uuid::Uuid::nil(),
            workspace_id: uuid::Uuid::nil(),
            actor_id: Some(uuid::Uuid::nil()),
            issue_id: uuid::Uuid::nil(),
            reaction: "heart".to_string(),
            a_first_name: Some("Ada".to_string()),
            a_last_name: Some("L".to_string()),
            a_avatar: Some("av".to_string()),
            a_avatar_asset_id: None,
            a_avatar_entity_type: None,
            a_is_bot: Some(false),
            a_display_name: Some("Ada L".to_string()),
        };
        let v = issue_reaction_json(&row);
        for key in [
            "id",
            "created_at",
            "updated_at",
            "created_by",
            "updated_by",
            "deleted_at",
            "project",
            "workspace",
            "actor",
            "issue",
            "reaction",
            "actor_detail",
        ] {
            assert!(v.get(key).is_some(), "IssueReaction missing key {key}");
        }
        assert_eq!(v.get("reaction"), Some(&json!("heart")));
        let detail = v.get("actor_detail").unwrap();
        for key in [
            "id",
            "first_name",
            "last_name",
            "avatar",
            "avatar_url",
            "is_bot",
            "display_name",
        ] {
            assert!(detail.get(key).is_some(), "actor_detail missing key {key}");
        }
    }

    #[test]
    fn ws_admin_fallback_covers_roleless_member() {
        // Same `project_gate_allows` shape as D5/D6/D7: a member with a
        // non-AMG role (or any membership) + ws-admin still passes.
        assert!(project_gate_allows(
            guard_reactions(Some(10)).is_ok(),
            true,
            true
        ));
        assert!(!project_gate_allows(
            guard_reactions(Some(10)).is_ok(),
            true,
            false
        ));
        assert!(!project_gate_allows(
            guard_reactions(None).is_ok(),
            false,
            true
        ));
    }
}
