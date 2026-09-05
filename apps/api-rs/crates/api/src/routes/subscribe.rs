use axum::{extract::State, http::StatusCode, Json};
use serde_json::{json, Value};

use crate::routes::project::{FORBIDDEN_MSG, deny, missing, user_avatar_url};
use crate::{middleware::auth::AuthUser, state::AppState};

use super::issue_common::fetch_project_member_role;

/// Quoted from `plane/app/views/issue/subscriber.py:77`.
pub(crate) const DUP_SUBSCRIBE_MSG: &str = "User already subscribed to the issue.";

/// Mirrors `subscription_status` (`subscriber.py:97-104`) → 200
/// `{"subscribed": bool}`.
pub(crate) fn subscribed_body(subscribed: bool) -> Value {
    json!({"subscribed": subscribed})
}

/// Gate for `subscribe` / `unsubscribe` / `subscription_status`: mirrors
/// `ProjectLitePermission` (`plane/app/permissions/project.py:136-146`) — any
/// ACTIVE project membership passes, with NO role filter (GUEST included —
/// differs from `ProjectEntityPermission`).
pub(crate) fn guard_lite(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(_) => Ok(()),
        None => Err(FORBIDDEN_MSG.to_string()),
    }
}

/// Gate for `DELETE issue-subscribers/:subscriber_id/`: mirrors the default
/// `ProjectEntityPermission` (`plane/app/permissions/project.py:96-119`) on
/// the unsafe (DELETE) branch — ADMIN (20) / MEMBER (15) only; GUEST (5) /
/// non-member → 403.
pub(crate) fn guard_subscriber_remove(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) => Ok(()),
        _ => Err(FORBIDDEN_MSG.to_string()),
    }
}

/// One `subscribers_list` row: the `ProjectMemberLiteSerializer` shape
/// (`plane/app/serializers/project.py:237-244` — `member` (UserLite), `id`,
/// `is_subscribed`). Field names match the SELECT aliases in
/// `subscribers_list`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct SubscriberMemberRow {
    pub(crate) id: uuid::Uuid,
    pub(crate) member_id: Option<uuid::Uuid>,
    pub(crate) u_first_name: Option<String>,
    pub(crate) u_last_name: Option<String>,
    pub(crate) u_avatar: Option<String>,
    pub(crate) u_avatar_asset_id: Option<uuid::Uuid>,
    pub(crate) u_avatar_entity_type: Option<String>,
    pub(crate) u_is_bot: Option<bool>,
    pub(crate) u_display_name: Option<String>,
    pub(crate) is_subscribed: Option<bool>,
}

/// Serializes one `SubscriberMemberRow` like `ProjectMemberLiteSerializer`
/// (`serializers/project.py:237-244`): `member` is the `UserLiteSerializer`
/// object (`serializers/user.py:141-153`: id, first_name, last_name, avatar,
/// avatar_url, is_bot, display_name) — null when the member FK is null — plus
/// the row `id` and `is_subscribed`.
///
/// Assumption (noted — Django passes NO annotation in `list`
/// (`subscriber.py:52-57`), so a bare `is_subscribed` read-only field has no
/// queryset value to read): `is_subscribed` is computed here as
/// `EXISTS(issue_subscribers for this issue + member)`.
pub(crate) fn subscriber_member_json(row: &SubscriberMemberRow) -> Value {
    let member = match row.member_id {
        Some(uid) => json!({
            "id": uid,
            "first_name": row.u_first_name,
            "last_name": row.u_last_name,
            "avatar": row.u_avatar,
            "avatar_url": user_avatar_url(
                row.u_avatar_asset_id,
                row.u_avatar_entity_type.as_deref(),
                row.u_avatar.as_deref(),
            ),
            "is_bot": row.u_is_bot,
            "display_name": row.u_display_name,
        }),
        None => Value::Null,
    };
    json!({
        "member": member,
        "id": row.id,
        "is_subscribed": row.is_subscribed.unwrap_or(false),
    })
}

/// One `subscribe` POST row: every `issue_subscribers` model column, matching
/// the `IssueSubscriberSerializer` (`serializers/issue.py:974-978`,
/// `fields = "__all__"` → id, created_at, updated_at, created_by,
/// updated_by, deleted_at, project, workspace, issue, subscriber). Field
/// names match the SELECT aliases in `subscribe`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct SubscriberRow {
    pub(crate) id: uuid::Uuid,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) updated_at: chrono::DateTime<chrono::Utc>,
    pub(crate) created_by_id: Option<uuid::Uuid>,
    pub(crate) updated_by_id: Option<uuid::Uuid>,
    pub(crate) deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) project_id: uuid::Uuid,
    pub(crate) workspace_id: uuid::Uuid,
    pub(crate) issue_id: uuid::Uuid,
    pub(crate) subscriber_id: uuid::Uuid,
}

/// Serializes one `SubscriberRow` like `IssueSubscriberSerializer`
/// (`serializers/issue.py:974-978`, `__all__`): FKs render as id strings
/// (DRF default PK representation), matching the batch convention
/// (datetimes RFC3339, keys sorted on the wire).
pub(crate) fn subscriber_json(row: &SubscriberRow) -> Value {
    let opt_id = |id: &Option<uuid::Uuid>| id.map(|u| json!(u)).unwrap_or(Value::Null);
    json!({
        "id": row.id,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "created_by": opt_id(&row.created_by_id),
        "updated_by": opt_id(&row.updated_by_id),
        "deleted_at": row.deleted_at,
        "project": row.project_id,
        "workspace": row.workspace_id,
        "issue": row.issue_id,
        "subscriber": row.subscriber_id,
    })
}

/// GET `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/subscribe/`
/// — parity with Django `subscription_status`
/// (`plane/app/views/issue/subscriber.py:97-104`,
/// `plane/app/urls/issue.py:185-189`).
///
/// - Gate: `ProjectLitePermission` — any ACTIVE project member (GUEST
///   included); non-member → 403 `deny()`.
/// - Django performs NO issue lookup here — a bare `EXISTS` filter over
///   (issue, subscriber=user, slug, project) → 200 `{"subscribed": bool}`,
///   even for an unknown issue id. Mirrored exactly (no 404 branch).
pub async fn subscription_status(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
    )>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    if guard_lite(role).is_err() {
        return Ok(deny());
    }
    let subscribed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issue_subscribers s JOIN workspaces w ON w.id = s.workspace_id \
          WHERE s.issue_id = $1 AND s.subscriber_id = $2 AND s.project_id = $3 \
          AND w.slug = $4 AND s.deleted_at IS NULL)",
    )
    .bind(issue_id)
    .bind(auth.0)
    .bind(project_id)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    Ok((StatusCode::OK, Json(subscribed_body(subscribed))))
}

/// POST `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/subscribe/`
/// — parity with Django `subscribe`
/// (`plane/app/views/issue/subscriber.py:69-85`,
/// `plane/app/urls/issue.py:185-189`).
///
/// - Gate: `ProjectLitePermission` (any ACTIVE member incl. GUEST).
/// - Duplicate (live row for issue + user) → 400
///   `{"message": "User already subscribed to the issue."}`
///   (`subscriber.py:76-79` — key `message`, NOT `error`).
/// - Else INSERTs the row (`subscriber.py:81-83`; `workspace` derived from
///   the URL slug exactly like `ProjectBaseModel.save`
///   (`project.py:187-189`) derives it from the project — the two agree
///   whenever slug matches the project's workspace; `created_by` = the
///   requester per `BaseModel.save`, `updated_by` stays NULL) → **201**
///   `IssueSubscriberSerializer` (`serializers/issue.py:974-978`, `__all__`).
///
/// Deviations: none on the wire — datetimes RFC3339 (batch convention).
pub async fn subscribe(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
    )>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    if guard_lite(role).is_err() {
        return Ok(deny());
    }
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issue_subscribers s JOIN workspaces w ON w.id = s.workspace_id \
          WHERE s.issue_id = $1 AND s.subscriber_id = $2 AND s.project_id = $3 \
          AND w.slug = $4 AND s.deleted_at IS NULL)",
    )
    .bind(issue_id)
    .bind(auth.0)
    .bind(project_id)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    if exists {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"message": DUP_SUBSCRIBE_MSG})),
        ));
    }
    let row: Option<SubscriberRow> = sqlx::query_as(
        "INSERT INTO issue_subscribers (id, issue_id, subscriber_id, project_id, workspace_id, created_by_id, created_at, updated_at) \
          SELECT gen_random_uuid(), $1, $2, $3, w.id, $2, now(), now() FROM workspaces w WHERE w.slug = $4 \
          RETURNING id, created_at, updated_at, created_by_id, updated_by_id, deleted_at, project_id, workspace_id, issue_id, subscriber_id",
    )
    .bind(issue_id)
    .bind(auth.0)
    .bind(project_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    // Unreachable when the gate passes (the gate's slug-scoped membership
    // lookup already proved the workspace exists), but Django would 500 on a
    // missing workspace while Rust returns the standard 404 instead.
    let Some(row) = row else {
        return Ok(missing());
    };
    Ok((StatusCode::CREATED, Json(subscriber_json(&row))))
}

/// DELETE `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/subscribe/`
/// — parity with Django `unsubscribe`
/// (`plane/app/views/issue/subscriber.py:87-95`,
/// `plane/app/urls/issue.py:185-189`).
///
/// - Gate: `ProjectLitePermission` (any ACTIVE member incl. GUEST).
/// - Django's `.get(project, subscriber=user, slug, issue)` miss raises
///   `DoesNotExist` → 404 `missing()` (`views/base.py:92-96`); the delete is
///   a soft-delete (`deleted_at=now()`, Django default-manager semantics) →
///   **204** empty.
pub async fn unsubscribe(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
    )>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    if guard_lite(role).is_err() {
        return Ok(deny());
    }
    let n = sqlx::query(
        "UPDATE issue_subscribers s SET deleted_at = now() FROM workspaces w \
          WHERE s.workspace_id = w.id AND w.slug = $1 AND s.project_id = $2 \
          AND s.issue_id = $3 AND s.subscriber_id = $4 AND s.deleted_at IS NULL",
    )
    .bind(&slug)
    .bind(project_id)
    .bind(issue_id)
    .bind(auth.0)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

/// GET `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-subscribers/`
/// — parity with Django `IssueSubscriberViewSet.list`
/// (`plane/app/views/issue/subscriber.py:52-57`,
/// `plane/app/urls/issue.py:174-179`).
///
/// - Gate: the viewset default `ProjectEntityPermission`
///   (`subscriber.py:20`) — GET is a safe method, so any ACTIVE project
///   member passes (role-agnostic, GUEST reads).
/// - Counterintuitively returns a `ProjectMemberLiteSerializer` LIST
///   (`serializers/project.py:237-244`: `member` (UserLite), `id`,
///   `is_subscribed`) over active project members
///   (`workspace__slug, project_id, is_active=True`), NOT the subscriber
///   rows — `Meta.ordering = ("-created_at",)` on `ProjectMember`.
/// - `is_subscribed` is computed per member as
///   `EXISTS(issue_subscribers for this issue + member)` (see
///   `subscriber_member_json` for the no-annotation assumption note).
pub async fn subscribers_list(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
    )>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    if role.is_none() {
        return Ok(deny());
    }
    let rows: Vec<SubscriberMemberRow> = sqlx::query_as(
        "SELECT pm.id, pm.member_id, \
          u.first_name AS u_first_name, u.last_name AS u_last_name, \
          u.avatar AS u_avatar, u.avatar_asset_id AS u_avatar_asset_id, \
          fa.entity_type AS u_avatar_entity_type, u.is_bot AS u_is_bot, \
          u.display_name AS u_display_name, \
          EXISTS(SELECT 1 FROM issue_subscribers s \
            WHERE s.issue_id = $3 AND s.subscriber_id = pm.member_id \
            AND s.deleted_at IS NULL) AS is_subscribed \
          FROM project_members pm \
          JOIN workspaces w ON w.id = pm.workspace_id \
          LEFT JOIN users u ON u.id = pm.member_id \
          LEFT JOIN file_assets fa ON fa.id = u.avatar_asset_id \
          WHERE w.slug = $1 AND pm.project_id = $2 \
          AND pm.is_active = true AND pm.deleted_at IS NULL \
          ORDER BY pm.created_at DESC",
    )
    .bind(&slug)
    .bind(project_id)
    .bind(issue_id)
    .fetch_all(&st.pool)
    .await?;
    let out: Vec<Value> = rows.iter().map(subscriber_member_json).collect();
    Ok((StatusCode::OK, Json(Value::Array(out))))
}

/// DELETE `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/issue-subscribers/:subscriber_id/`
/// — parity with Django `IssueSubscriberViewSet.destroy`
/// (`plane/app/views/issue/subscriber.py:59-67`,
/// `plane/app/urls/issue.py:180-184`).
///
/// - Gate: the viewset default `ProjectEntityPermission` on the unsafe
///   (DELETE) branch — ADMIN (20) / MEMBER (15) only; GUEST / non-member →
///   403 `deny()`.
/// - `:subscriber_id` is the SUBSCRIBER USER id (Django filters
///   `subscriber=subscriber_id`, `subscriber.py:60-65`), not the
///   `IssueSubscriber` row id. Miss → 404 `missing()` (Django `.get()`
///   → `DoesNotExist` → `views/base.py:92-96`); success is a soft-delete →
///   **204** empty.
pub async fn subscriber_remove(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id, subscriber_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
        uuid::Uuid,
    )>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    if guard_subscriber_remove(role).is_err() {
        return Ok(deny());
    }
    let n = sqlx::query(
        "UPDATE issue_subscribers s SET deleted_at = now() FROM workspaces w \
          WHERE s.workspace_id = w.id AND w.slug = $1 AND s.project_id = $2 \
          AND s.issue_id = $3 AND s.subscriber_id = $4 AND s.deleted_at IS NULL",
    )
    .bind(&slug)
    .bind(project_id)
    .bind(issue_id)
    .bind(subscriber_id)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

#[cfg(test)]
mod batch_d_d1_tests {
    use super::*;

    #[test]
    fn dup_subscribe_message_matches_django() {
        // Quoted from `plane/app/views/issue/subscriber.py:77`.
        assert_eq!(DUP_SUBSCRIBE_MSG, "User already subscribed to the issue.");
    }

    #[test]
    fn subscribed_body_shapes_match_django() {
        // Mirrors `subscription_status` (`subscriber.py:97-104`) → 200
        // `{"subscribed": bool}`.
        assert_eq!(subscribed_body(true), serde_json::json!({"subscribed": true}));
        assert_eq!(subscribed_body(false), serde_json::json!({"subscribed": false}));
    }

    #[test]
    fn lite_gate_passes_any_active_member_incl_guest() {
        // Mirrors `ProjectLitePermission` (`permissions/project.py:136-146`):
        // any ACTIVE project membership passes — no role filter, so GUEST
        // (5) passes exactly like ADMIN (20) / MEMBER (15).
        assert!(guard_lite(Some(20)).is_ok());
        assert!(guard_lite(Some(15)).is_ok());
        assert!(guard_lite(Some(5)).is_ok());
        assert_eq!(
            guard_lite(None).unwrap_err(),
            "You don't have the required permissions."
        );
    }

    #[test]
    fn subscriber_remove_gate_matches_entity_unsafe_branch() {
        // Mirrors the default `ProjectEntityPermission` unsafe branch
        // (`permissions/project.py:112-119`): ADMIN/MEMBER only.
        assert!(guard_subscriber_remove(Some(20)).is_ok());
        assert!(guard_subscriber_remove(Some(15)).is_ok());
        assert!(guard_subscriber_remove(Some(5)).is_err());
        assert!(guard_subscriber_remove(None).is_err());
    }

    #[test]
    fn subscriber_member_json_matches_project_member_lite() {
        // Mirrors `ProjectMemberLiteSerializer`
        // (`serializers/project.py:237-244`): exactly the keys `member`,
        // `id`, `is_subscribed`, with `member` as the 7-key UserLite object.
        let row = SubscriberMemberRow {
            id: uuid::Uuid::nil(),
            member_id: Some(uuid::Uuid::nil()),
            u_first_name: Some("Ada".to_string()),
            u_last_name: Some("L".to_string()),
            u_avatar: Some("av".to_string()),
            u_avatar_asset_id: None,
            u_avatar_entity_type: None,
            u_is_bot: Some(false),
            u_display_name: Some("Ada L".to_string()),
            is_subscribed: Some(true),
        };
        let v = subscriber_member_json(&row);
        assert!(v.get("member").is_some());
        assert!(v.get("id").is_some());
        assert_eq!(v.get("is_subscribed"), Some(&serde_json::json!(true)));
        let member = v.get("member").unwrap();
        for key in ["id", "first_name", "last_name", "avatar", "avatar_url", "is_bot", "display_name"] {
            assert!(member.get(key).is_some(), "UserLite missing key {key}");
        }
        // NULL member FK renders `member: null` (Django renders the nested
        // read-only serializer as null).
        let null_row = SubscriberMemberRow {
            member_id: None,
            u_first_name: None,
            u_last_name: None,
            u_avatar: None,
            u_avatar_asset_id: None,
            u_avatar_entity_type: None,
            u_is_bot: None,
            u_display_name: None,
            is_subscribed: None,
            ..row
        };
        let nv = subscriber_member_json(&null_row);
        assert!(nv.get("member").unwrap().is_null());
        assert_eq!(nv.get("is_subscribed"), Some(&serde_json::json!(false)));
    }

    #[test]
    fn subscriber_json_covers_all_serializer_keys() {
        // Mirrors `IssueSubscriberSerializer` (`serializers/issue.py:974-978`,
        // `fields = "__all__"`): all 10 model columns.
        let row = SubscriberRow {
            id: uuid::Uuid::nil(),
            created_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            updated_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            created_by_id: Some(uuid::Uuid::nil()),
            updated_by_id: None,
            deleted_at: None,
            project_id: uuid::Uuid::nil(),
            workspace_id: uuid::Uuid::nil(),
            issue_id: uuid::Uuid::nil(),
            subscriber_id: uuid::Uuid::nil(),
        };
        let v = subscriber_json(&row);
        for key in [
            "id",
            "created_at",
            "updated_at",
            "created_by",
            "updated_by",
            "deleted_at",
            "project",
            "workspace",
            "issue",
            "subscriber",
        ] {
            assert!(v.get(key).is_some(), "IssueSubscriber missing key {key}");
        }
        assert!(v.get("updated_by").unwrap().is_null());
        assert!(v.get("deleted_at").unwrap().is_null());
    }
}
