use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::routes::project::{cover_image_url, deny, missing, user_avatar_url, workspace_logo_url};
use crate::{middleware::auth::AuthUser, state::AppState};

use super::issue_common::{fetch_project_member_role, is_workspace_admin, project_gate_allows};
use super::issue_query::parse_deleted_updated_at_gt;

/// Issue history (`history/`) + issue meta (`meta/`) — parity with Django
/// `IssueActivityEndpoint` (`plane/app/views/issue/activity.py:24-86`,
/// `plane/app/urls/issue.py:149-153`) and `IssueMetaEndpoint`
/// (`plane/app/views/issue/base.py:1186-1198`,
/// `plane/app/urls/issue.py:277-279`). Issues path ONLY (Django defines no
/// work-items/epics variants).
///
/// Live columns verified 2026-09-06 via
/// `docker exec plane-db psql -U plane -d plane -c "\d issue_activities"` /
/// `"\d issue_comments"`: `issue_activities` has NO `activity_type` column —
/// `?activity_type=` is a view-level switch only (pure fn `history_branch`
/// below), and `field` is nullable (Django `~Q(field__in=...)` drops NULL
/// fields in SQL; the mirrored `NOT IN` below has identical NULL
/// semantics). Celery/activity side-effects skipped (Batch C precedent —
/// read-only GETs here anyway).
///
/// Comment-shape note (per plan): the `work_item.rs` comment struct covers
/// only `{id, comment_html}`, while Django's `IssueCommentSerializer`
/// (`serializers/issue.py:697-707`) is model `__all__` + `actor_detail`,
/// `issue_detail`, `project_detail`, `workspace_detail`,
/// `comment_reactions`, `is_member` — a strict superset, so this file
/// defines NEW row/JSON builders instead of reusing that struct. `is_member`
/// is OMITTED from the JSON: this view annotates no `is_member`
/// (`activity.py:47-64`, unlike `comment.py:51-57`), so DRF's missing
/// attribute on the non-required `BooleanField` raises `SkipField` and the
/// key is absent on the wire. `comment_reactions` IS prefetched here
/// (`activity.py:58-63`), so nested reactions are included (newest first,
/// `CommentReaction` `Meta.ordering = ("-created_at",)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistoryBranch {
    Properties,
    Comments,
    Merged,
}

/// Mirrors the `activity_type` switch (`activity.py:66-86`): exact
/// `"issue-property"` / `"issue-comment"` take the single-list branches;
/// ANY other value (incl. missing) falls through to the merged default
/// (`activity.py:81-86`).
pub(crate) fn history_branch(raw: Option<&str>) -> HistoryBranch {
    match raw {
        Some("issue-property") => HistoryBranch::Properties,
        Some("issue-comment") => HistoryBranch::Comments,
        _ => HistoryBranch::Merged,
    }
}

/// Query params for `history`: mirrors `request.GET.get("activity_type")`
/// and `request.GET.get("created_at__gt")` (`activity.py:32-33,66-77`).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct HistoryQuery {
    #[serde(default)]
    pub activity_type: Option<String>,
    #[serde(default, rename = "created_at__gt")]
    pub created_at_gt: Option<String>,
}

/// Shared PROJECT-level gate for both handlers: `ProjectEntityPermission`
/// on a safe (GET) method passes any ACTIVE project member
/// (`permissions/project.py:103-110`), and `@allow_permission([ADMIN,
/// MEMBER, GUEST])` (`activity.py:29`, `base.py:1187`) passes roles
/// 20/15/5 outright with the workspace-ADMIN fallback
/// (`permissions/base.py:53-78`) — exactly the `list_by_ids` gate, reused
/// via the same shared helpers.
async fn gate(
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

fn opt_id(id: &Option<uuid::Uuid>) -> Value {
    id.map(|u| json!(u)).unwrap_or(Value::Null)
}

/// One `UserLiteSerializer` object (`serializers/user.py:141-153`: id,
/// first_name, last_name, avatar, avatar_url, is_bot, display_name) — null
/// when the FK is null. Same shape as `subscriber_member_json` (D1).
#[allow(clippy::too_many_arguments)]
pub(crate) fn actor_detail_json(
    id: Option<uuid::Uuid>,
    first_name: &Option<String>,
    last_name: &Option<String>,
    avatar: &Option<String>,
    avatar_asset_id: Option<uuid::Uuid>,
    avatar_entity_type: Option<&str>,
    is_bot: &Option<bool>,
    display_name: &Option<String>,
) -> Value {
    match id {
        Some(uid) => json!({
            "id": uid,
            "first_name": first_name,
            "last_name": last_name,
            "avatar": avatar,
            "avatar_url": user_avatar_url(avatar_asset_id, avatar_entity_type, avatar.as_deref()),
            "is_bot": is_bot,
            "display_name": display_name,
        }),
        None => Value::Null,
    }
}

/// One `IssueFlatSerializer` object (`serializers/issue.py:52-69`): id,
/// name, description_json, description_html, priority, start_date,
/// target_date, sequence_id, sort_order, is_draft — null when the FK is
/// null (activities carry a nullable `issue`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn issue_flat_json(
    id: Option<uuid::Uuid>,
    name: &Option<String>,
    description_json: Option<&Value>,
    description_html: &Option<String>,
    priority: &Option<String>,
    start_date: &Option<chrono::NaiveDate>,
    target_date: &Option<chrono::NaiveDate>,
    sequence_id: Option<i32>,
    sort_order: Option<f64>,
    is_draft: Option<bool>,
) -> Value {
    match id {
        Some(uid) => json!({
            "id": uid,
            "name": name,
            "description_json": description_json.cloned().unwrap_or(Value::Null),
            "description_html": description_html,
            "priority": priority,
            "start_date": start_date,
            "target_date": target_date,
            "sequence_id": sequence_id,
            "sort_order": sort_order,
            "is_draft": is_draft,
        }),
        None => Value::Null,
    }
}

/// One `ProjectLiteSerializer` object
/// (`serializers/project.py:100-111`): id, identifier, name, cover_image,
/// cover_image_url, logo_props, description. Same shape as the `project`
/// object in `my_membership_json`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn project_lite_json(
    id: uuid::Uuid,
    identifier: &str,
    name: &str,
    cover_image: &Option<String>,
    cover_asset_id: Option<uuid::Uuid>,
    cover_entity_type: Option<&str>,
    logo_props: &Value,
    description: &str,
) -> Value {
    json!({
        "id": id,
        "identifier": identifier,
        "name": name,
        "cover_image": cover_image,
        "cover_image_url": cover_image_url(cover_asset_id, cover_entity_type, cover_image.as_deref()),
        "logo_props": logo_props,
        "description": description,
    })
}

/// One `WorkspaceLiteSerializer` object
/// (`serializers/workspace.py:86-90`): name, slug, id, logo_url. Same
/// shape as the `workspace` object in `my_membership_json`.
pub(crate) fn workspace_lite_json(
    id: uuid::Uuid,
    name: &str,
    slug: &str,
    logo: &Option<String>,
    logo_asset_id: Option<uuid::Uuid>,
    logo_entity_type: Option<&str>,
) -> Value {
    json!({
        "name": name,
        "slug": slug,
        "id": id,
        "logo_url": workspace_logo_url(logo_asset_id, logo_entity_type, logo.as_deref()),
    })
}

/// One `issue_activities` row plus the columns for the four nested lite
/// serializers. Field names match the SELECT aliases in `history`.
/// `src_*` are populated ONLY for the `issue-property` branch (see
/// `fetch_source_data`); otherwise None → `"source_data": null`, mirroring
/// `get_source_data` (`serializers/issue.py:340-347`), whose `hasattr`
/// guards yield None without the `source_data` prefetch.
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct ActivityRow {
    pub(crate) id: uuid::Uuid,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) updated_at: chrono::DateTime<chrono::Utc>,
    pub(crate) created_by_id: Option<uuid::Uuid>,
    pub(crate) updated_by_id: Option<uuid::Uuid>,
    pub(crate) deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) project_id: uuid::Uuid,
    pub(crate) workspace_id: uuid::Uuid,
    pub(crate) issue_id: Option<uuid::Uuid>,
    pub(crate) verb: String,
    pub(crate) field: Option<String>,
    pub(crate) old_value: Option<String>,
    pub(crate) new_value: Option<String>,
    pub(crate) comment: String,
    pub(crate) attachments: Vec<String>,
    pub(crate) issue_comment_id: Option<uuid::Uuid>,
    pub(crate) actor_id: Option<uuid::Uuid>,
    pub(crate) old_identifier: Option<uuid::Uuid>,
    pub(crate) new_identifier: Option<uuid::Uuid>,
    pub(crate) epoch: Option<f64>,
    pub(crate) a_id: Option<uuid::Uuid>,
    pub(crate) a_first_name: Option<String>,
    pub(crate) a_last_name: Option<String>,
    pub(crate) a_avatar: Option<String>,
    pub(crate) a_avatar_asset_id: Option<uuid::Uuid>,
    pub(crate) a_avatar_entity_type: Option<String>,
    pub(crate) a_is_bot: Option<bool>,
    pub(crate) a_display_name: Option<String>,
    pub(crate) i_id: Option<uuid::Uuid>,
    pub(crate) i_name: Option<String>,
    pub(crate) i_description_json: Option<Value>,
    pub(crate) i_description_html: Option<String>,
    pub(crate) i_priority: Option<String>,
    pub(crate) i_start_date: Option<chrono::NaiveDate>,
    pub(crate) i_target_date: Option<chrono::NaiveDate>,
    pub(crate) i_sequence_id: Option<i32>,
    pub(crate) i_sort_order: Option<f64>,
    pub(crate) i_is_draft: Option<bool>,
    pub(crate) p_id: uuid::Uuid,
    pub(crate) p_identifier: String,
    pub(crate) p_name: String,
    pub(crate) p_cover_image: Option<String>,
    pub(crate) p_cover_asset_id: Option<uuid::Uuid>,
    pub(crate) p_cover_entity_type: Option<String>,
    pub(crate) p_logo_props: Value,
    pub(crate) p_description: String,
    pub(crate) w_id: uuid::Uuid,
    pub(crate) w_name: String,
    pub(crate) w_slug: String,
    pub(crate) w_logo: Option<String>,
    pub(crate) w_logo_asset_id: Option<uuid::Uuid>,
    pub(crate) w_logo_entity_type: Option<String>,
}

/// Serializes one `ActivityRow` like `IssueActivitySerializer`
/// (`serializers/issue.py:333-351`): model `__all__` (FKs as id strings,
/// DRF's default PK representation) + `actor_detail`, `issue_detail`,
/// `project_detail`, `workspace_detail`, `source_data`.
///
/// Key-order note: model `__all__` keys first, then the extras (repo
/// precedent — `my_membership_json`); the wire KEY SET matches Django
/// exactly, order is a documented non-divergence (serde `Value` equality
/// is order-insensitive).
pub(crate) fn activity_json(row: &ActivityRow, source_data: Option<Value>) -> Value {
    json!({
        "id": row.id,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "created_by": opt_id(&row.created_by_id),
        "updated_by": opt_id(&row.updated_by_id),
        "deleted_at": row.deleted_at,
        "project": row.project_id,
        "workspace": row.workspace_id,
        "issue": opt_id(&row.issue_id),
        "verb": row.verb,
        "field": row.field,
        "old_value": row.old_value,
        "new_value": row.new_value,
        "comment": row.comment,
        "attachments": row.attachments,
        "issue_comment": opt_id(&row.issue_comment_id),
        "actor": opt_id(&row.actor_id),
        "old_identifier": opt_id(&row.old_identifier),
        "new_identifier": opt_id(&row.new_identifier),
        "epoch": row.epoch,
        "actor_detail": actor_detail_json(
            row.a_id, &row.a_first_name, &row.a_last_name, &row.a_avatar,
            row.a_avatar_asset_id, row.a_avatar_entity_type.as_deref(),
            &row.a_is_bot, &row.a_display_name,
        ),
        "issue_detail": issue_flat_json(
            row.i_id, &row.i_name, row.i_description_json.as_ref(),
            &row.i_description_html, &row.i_priority, &row.i_start_date,
            &row.i_target_date, row.i_sequence_id, row.i_sort_order, row.i_is_draft,
        ),
        "project_detail": project_lite_json(
            row.p_id, &row.p_identifier, &row.p_name, &row.p_cover_image,
            row.p_cover_asset_id, row.p_cover_entity_type.as_deref(),
            &row.p_logo_props, &row.p_description,
        ),
        "workspace_detail": workspace_lite_json(
            row.w_id, &row.w_name, &row.w_slug, &row.w_logo,
            row.w_logo_asset_id, row.w_logo_entity_type.as_deref(),
        ),
        "source_data": source_data.unwrap_or(Value::Null),
    })
}

/// One `issue_comments` row plus the nested-lite columns. Field names match
/// the SELECT aliases in `history`. (`description` is the nullable
/// `description_id` OneToOne; `parent` the nullable self-FK.)
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct CommentRow {
    pub(crate) id: uuid::Uuid,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) updated_at: chrono::DateTime<chrono::Utc>,
    pub(crate) created_by_id: Option<uuid::Uuid>,
    pub(crate) updated_by_id: Option<uuid::Uuid>,
    pub(crate) deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) workspace_id: uuid::Uuid,
    pub(crate) project_id: uuid::Uuid,
    pub(crate) comment_stripped: Option<String>,
    pub(crate) comment_json: Value,
    pub(crate) comment_html: String,
    pub(crate) description_id: Option<uuid::Uuid>,
    pub(crate) attachments: Vec<String>,
    pub(crate) issue_id: uuid::Uuid,
    pub(crate) actor_id: Option<uuid::Uuid>,
    pub(crate) access: String,
    pub(crate) external_source: Option<String>,
    pub(crate) external_id: Option<String>,
    pub(crate) edited_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) parent_id: Option<uuid::Uuid>,
    pub(crate) a_id: Option<uuid::Uuid>,
    pub(crate) a_first_name: Option<String>,
    pub(crate) a_last_name: Option<String>,
    pub(crate) a_avatar: Option<String>,
    pub(crate) a_avatar_asset_id: Option<uuid::Uuid>,
    pub(crate) a_avatar_entity_type: Option<String>,
    pub(crate) a_is_bot: Option<bool>,
    pub(crate) a_display_name: Option<String>,
    pub(crate) i_id: Option<uuid::Uuid>,
    pub(crate) i_name: Option<String>,
    pub(crate) i_description_json: Option<Value>,
    pub(crate) i_description_html: Option<String>,
    pub(crate) i_priority: Option<String>,
    pub(crate) i_start_date: Option<chrono::NaiveDate>,
    pub(crate) i_target_date: Option<chrono::NaiveDate>,
    pub(crate) i_sequence_id: Option<i32>,
    pub(crate) i_sort_order: Option<f64>,
    pub(crate) i_is_draft: Option<bool>,
    pub(crate) p_id: uuid::Uuid,
    pub(crate) p_identifier: String,
    pub(crate) p_name: String,
    pub(crate) p_cover_image: Option<String>,
    pub(crate) p_cover_asset_id: Option<uuid::Uuid>,
    pub(crate) p_cover_entity_type: Option<String>,
    pub(crate) p_logo_props: Value,
    pub(crate) p_description: String,
    pub(crate) w_id: uuid::Uuid,
    pub(crate) w_name: String,
    pub(crate) w_slug: String,
    pub(crate) w_logo: Option<String>,
    pub(crate) w_logo_asset_id: Option<uuid::Uuid>,
    pub(crate) w_logo_entity_type: Option<String>,
}

/// One nested `comment_reactions` entry: `CommentReactionSerializer`
/// (`serializers/issue.py:666-685`): id, actor, comment, reaction,
/// display_name (from `actor.display_name`, null when actor null),
/// deleted_at, workspace, project, created_at, updated_at, created_by,
/// updated_by.
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct CommentReactionRow {
    pub(crate) id: uuid::Uuid,
    pub(crate) actor_id: Option<uuid::Uuid>,
    pub(crate) comment_id: uuid::Uuid,
    pub(crate) reaction: String,
    pub(crate) actor_display_name: Option<String>,
    pub(crate) deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) workspace_id: uuid::Uuid,
    pub(crate) project_id: uuid::Uuid,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) updated_at: chrono::DateTime<chrono::Utc>,
    pub(crate) created_by_id: Option<uuid::Uuid>,
    pub(crate) updated_by_id: Option<uuid::Uuid>,
}

pub(crate) fn comment_reaction_json(row: &CommentReactionRow) -> Value {
    json!({
        "id": row.id,
        "actor": opt_id(&row.actor_id),
        "comment": row.comment_id,
        "reaction": row.reaction,
        "display_name": row.actor_display_name,
        "deleted_at": row.deleted_at,
        "workspace": row.workspace_id,
        "project": row.project_id,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "created_by": opt_id(&row.created_by_id),
        "updated_by": opt_id(&row.updated_by_id),
    })
}

/// Serializes one `CommentRow` like `IssueCommentSerializer`
/// (`serializers/issue.py:697-707`): model `__all__` + the four `*_detail`
/// objects + `comment_reactions`. `is_member` is deliberately ABSENT (see
/// module docs: DRF `SkipField`, `activity.py` annotates nothing).
pub(crate) fn comment_json(row: &CommentRow, reactions: Vec<Value>) -> Value {
    json!({
        "id": row.id,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "created_by": opt_id(&row.created_by_id),
        "updated_by": opt_id(&row.updated_by_id),
        "deleted_at": row.deleted_at,
        "workspace": row.workspace_id,
        "project": row.project_id,
        "comment_stripped": row.comment_stripped,
        "comment_json": row.comment_json,
        "comment_html": row.comment_html,
        "description": opt_id(&row.description_id),
        "attachments": row.attachments,
        "issue": row.issue_id,
        "actor": opt_id(&row.actor_id),
        "access": row.access,
        "external_source": row.external_source,
        "external_id": row.external_id,
        "edited_at": row.edited_at,
        "parent": opt_id(&row.parent_id),
        "actor_detail": actor_detail_json(
            row.a_id, &row.a_first_name, &row.a_last_name, &row.a_avatar,
            row.a_avatar_asset_id, row.a_avatar_entity_type.as_deref(),
            &row.a_is_bot, &row.a_display_name,
        ),
        "issue_detail": issue_flat_json(
            row.i_id, &row.i_name, row.i_description_json.as_ref(),
            &row.i_description_html, &row.i_priority, &row.i_start_date,
            &row.i_target_date, row.i_sequence_id, row.i_sort_order, row.i_is_draft,
        ),
        "project_detail": project_lite_json(
            row.p_id, &row.p_identifier, &row.p_name, &row.p_cover_image,
            row.p_cover_asset_id, row.p_cover_entity_type.as_deref(),
            &row.p_logo_props, &row.p_description,
        ),
        "workspace_detail": workspace_lite_json(
            row.w_id, &row.w_name, &row.w_slug, &row.w_logo,
            row.w_logo_asset_id, row.w_logo_entity_type.as_deref(),
        ),
        "comment_reactions": reactions,
    })
}

const ACTIVITY_SELECT: &str = "a.id, a.created_at, a.updated_at, \
    a.created_by_id, a.updated_by_id, a.deleted_at, \
    a.project_id, a.workspace_id, a.issue_id, a.verb, a.field, \
    a.old_value, a.new_value, a.comment, a.attachments, \
    a.issue_comment_id, a.actor_id, a.old_identifier, a.new_identifier, a.epoch, \
    u.id AS a_id, u.first_name AS a_first_name, u.last_name AS a_last_name, \
    u.avatar AS a_avatar, u.avatar_asset_id AS a_avatar_asset_id, \
    fa_u.entity_type AS a_avatar_entity_type, u.is_bot AS a_is_bot, \
    u.display_name AS a_display_name, \
    i.id AS i_id, i.name AS i_name, i.description_json AS i_description_json, \
    i.description_html AS i_description_html, i.priority AS i_priority, \
    i.start_date AS i_start_date, i.target_date AS i_target_date, \
    i.sequence_id AS i_sequence_id, i.sort_order AS i_sort_order, \
    i.is_draft AS i_is_draft, \
    p.id AS p_id, p.identifier AS p_identifier, p.name AS p_name, \
    p.cover_image AS p_cover_image, p.cover_image_asset_id AS p_cover_asset_id, \
    fa_p.entity_type AS p_cover_entity_type, p.logo_props AS p_logo_props, \
    p.description AS p_description, \
    w.id AS w_id, w.name AS w_name, w.slug AS w_slug, w.logo AS w_logo, \
    w.logo_asset_id AS w_logo_asset_id, fa_w.entity_type AS w_logo_entity_type";

const ACTIVITY_JOINS: &str = "FROM issue_activities a \
    JOIN projects p ON p.id = a.project_id \
    JOIN workspaces w ON w.id = a.workspace_id \
    LEFT JOIN users u ON u.id = a.actor_id \
    LEFT JOIN file_assets fa_u ON fa_u.id = u.avatar_asset_id \
    LEFT JOIN issues i ON i.id = a.issue_id \
    LEFT JOIN file_assets fa_p ON fa_p.id = p.cover_image_asset_id \
    LEFT JOIN file_assets fa_w ON fa_w.id = w.logo_asset_id";

/// Fetches the activity queryset (`activity.py:35-46`): this issue, field
/// NOT IN (comment, vote, reaction, draft) — NULL fields drop out exactly
/// like Django's `~Q(field__in=...)` — live rows only (soft-delete default
/// managers are implicit in Django), project not archived, slug-scoped,
/// optional `created_at__gt`, ordering ASCENDING (`order_by("created_at")`).
async fn fetch_activities(
    pool: &sqlx::PgPool,
    slug: &str,
    project_id: uuid::Uuid,
    issue_id: uuid::Uuid,
    created_gt: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<Vec<ActivityRow>, sqlx::Error> {
    let gt_filter = if created_gt.is_some() {
        "AND a.created_at > $4"
    } else {
        ""
    };
    let sql = format!(
        "SELECT {ACTIVITY_SELECT} {ACTIVITY_JOINS} \
        WHERE a.issue_id = $1 AND a.project_id = $2 AND w.slug = $3 \
        AND a.deleted_at IS NULL \
        AND a.field NOT IN ('comment', 'vote', 'reaction', 'draft') \
        AND p.archived_at IS NULL AND p.deleted_at IS NULL \
        {gt_filter} \
        ORDER BY a.created_at ASC"
    );
    let mut q = sqlx::query_as::<_, ActivityRow>(&sql)
        .bind(issue_id)
        .bind(project_id)
        .bind(slug);
    if let Some(gt) = created_gt {
        q = q.bind(gt);
    }
    q.fetch_all(pool).await
}

/// Mirrors the `issue-property` prefetch (`activity.py:67-73`):
/// `issue__issue_intake` (related_name on `IntakeIssue.issue`,
/// `db/models/intake.py:52`) `.only("source_email", "source", "extra")`.
/// `source_data[0]` follows `IntakeIssue` `Meta.ordering =
/// ("-created_at",)` (`intake.py:80`) — newest first — so `ORDER BY
/// created_at DESC LIMIT 1`. All rows here share one issue, so a single
/// lookup serves every activity; None → `"source_data": null` (Django's
/// `get_source_data` falls to None).
async fn fetch_source_data(
    pool: &sqlx::PgPool,
    issue_id: uuid::Uuid,
) -> Result<Option<Value>, sqlx::Error> {
    let row: Option<(Option<String>, Option<String>, Value)> = sqlx::query_as(
        "SELECT source, source_email, extra FROM intake_issues \
        WHERE issue_id = $1 AND deleted_at IS NULL \
        ORDER BY created_at DESC LIMIT 1",
    )
    .bind(issue_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(source, source_email, extra)| {
        // Key order mirrors `get_source_data` (`serializers/issue.py:340-347`).
        json!({"source": source, "source_email": source_email, "extra": extra})
    }))
}

const COMMENT_SELECT: &str = "c.id, c.created_at, c.updated_at, \
    c.created_by_id, c.updated_by_id, c.deleted_at, \
    c.workspace_id, c.project_id, c.comment_stripped, c.comment_json, \
    c.comment_html, c.description_id, c.attachments, c.issue_id, c.actor_id, \
    c.access, c.external_source, c.external_id, c.edited_at, c.parent_id, \
    u.id AS a_id, u.first_name AS a_first_name, u.last_name AS a_last_name, \
    u.avatar AS a_avatar, u.avatar_asset_id AS a_avatar_asset_id, \
    fa_u.entity_type AS a_avatar_entity_type, u.is_bot AS a_is_bot, \
    u.display_name AS a_display_name, \
    i.id AS i_id, i.name AS i_name, i.description_json AS i_description_json, \
    i.description_html AS i_description_html, i.priority AS i_priority, \
    i.start_date AS i_start_date, i.target_date AS i_target_date, \
    i.sequence_id AS i_sequence_id, i.sort_order AS i_sort_order, \
    i.is_draft AS i_is_draft, \
    p.id AS p_id, p.identifier AS p_identifier, p.name AS p_name, \
    p.cover_image AS p_cover_image, p.cover_image_asset_id AS p_cover_asset_id, \
    fa_p.entity_type AS p_cover_entity_type, p.logo_props AS p_logo_props, \
    p.description AS p_description, \
    w.id AS w_id, w.name AS w_name, w.slug AS w_slug, w.logo AS w_logo, \
    w.logo_asset_id AS w_logo_asset_id, fa_w.entity_type AS w_logo_entity_type";

/// Fetches the comment queryset (`activity.py:47-64`): this issue, live
/// rows, project not archived, slug-scoped, optional `created_at__gt`,
/// ordering ASCENDING. (The `comment_reactions` prefetch is mirrored by
/// `fetch_comment_reactions`, newest first.)
async fn fetch_comments(
    pool: &sqlx::PgPool,
    slug: &str,
    project_id: uuid::Uuid,
    issue_id: uuid::Uuid,
    created_gt: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<Vec<CommentRow>, sqlx::Error> {
    let gt_filter = if created_gt.is_some() {
        "AND c.created_at > $4"
    } else {
        ""
    };
    let sql = format!(
        "SELECT {COMMENT_SELECT} \
        FROM issue_comments c \
        JOIN projects p ON p.id = c.project_id \
        JOIN workspaces w ON w.id = c.workspace_id \
        LEFT JOIN users u ON u.id = c.actor_id \
        LEFT JOIN file_assets fa_u ON fa_u.id = u.avatar_asset_id \
        JOIN issues i ON i.id = c.issue_id \
        LEFT JOIN file_assets fa_p ON fa_p.id = p.cover_image_asset_id \
        LEFT JOIN file_assets fa_w ON fa_w.id = w.logo_asset_id \
        WHERE c.issue_id = $1 AND c.project_id = $2 AND w.slug = $3 \
        AND c.deleted_at IS NULL \
        AND p.archived_at IS NULL AND p.deleted_at IS NULL \
        {gt_filter} \
        ORDER BY c.created_at ASC"
    );
    let mut q = sqlx::query_as::<_, CommentRow>(&sql)
        .bind(issue_id)
        .bind(project_id)
        .bind(slug);
    if let Some(gt) = created_gt {
        q = q.bind(gt);
    }
    q.fetch_all(pool).await
}

/// Mirrors the `Prefetch("comment_reactions",
/// queryset=...select_related("actor"))` (`activity.py:58-63`): live
/// reactions for the given comments, newest first (`CommentReaction`
/// `Meta.ordering = ("-created_at",)`, `db/models/issue.py:648`).
async fn fetch_comment_reactions(
    pool: &sqlx::PgPool,
    comment_ids: &[uuid::Uuid],
) -> Result<Vec<CommentReactionRow>, sqlx::Error> {
    if comment_ids.is_empty() {
        return Ok(vec![]);
    }
    sqlx::query_as::<_, CommentReactionRow>(
        "SELECT r.id, r.actor_id, r.comment_id, r.reaction, \
        u.display_name AS actor_display_name, r.deleted_at, \
        r.workspace_id, r.project_id, r.created_at, r.updated_at, \
        r.created_by_id, r.updated_by_id \
        FROM comment_reactions r LEFT JOIN users u ON u.id = r.actor_id \
        WHERE r.comment_id = ANY($1) AND r.deleted_at IS NULL \
        ORDER BY r.created_at DESC",
    )
    .bind(comment_ids)
    .fetch_all(pool)
    .await
}

/// GET `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/history/`
/// — parity with Django `IssueActivityEndpoint.get`
/// (`plane/app/views/issue/activity.py:30-86`).
///
/// - Gate: PROJECT ADMIN/MEMBER/GUEST via the shared helpers (see `gate`).
/// - `?activity_type=issue-property` → `IssueActivitySerializer[]` (with
///   the `source_data` prefetch); `?activity_type=issue-comment` →
///   `IssueCommentSerializer[]`; ANY other value (incl. missing) → merged
///   list, `sorted(chain(...))` ASCENDING by `created_at`
///   (`activity.py:81-84`) — the stable sort over activities-then-comments
///   keeps activities first on ties, exactly like Python's stable `sorted`
///   over `chain(activities, comments)`.
/// - `?created_at__gt` filters both querysets (`__gt`); garbage → 400
///   `{"error": "Please provide valid detail"}` (same `DateTimeField`
///   lookup semantics as the reused `parse_deleted_updated_at_gt`).
/// - Activities exclude `field IN (comment, vote, reaction, draft)`;
///   `source_data` is prefetched ONLY for the issue-property branch
///   (merged-branch activities render `"source_data": null`).
/// - No issue-existence check: Django performs none (unknown issue → 200
///   `[]`), mirrored here.
///
/// Deviations: datetimes RFC3339 (chrono, batch convention) vs DRF
/// per-user-timezone ISO8601; JSON key ORDER is model-keys-first (repo
/// precedent) while the wire KEY SET matches Django exactly.
pub async fn history(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
    )>,
    axum::extract::Query(params): axum::extract::Query<HistoryQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let created_gt = match params.created_at_gt.as_deref() {
        Some(raw) => match parse_deleted_updated_at_gt(raw) {
            Ok(dt) => Some(dt),
            Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e})))),
        },
        None => None,
    };
    match history_branch(params.activity_type.as_deref()) {
        HistoryBranch::Properties => {
            let rows = fetch_activities(&st.pool, &slug, project_id, issue_id, created_gt).await?;
            let source = fetch_source_data(&st.pool, issue_id).await?;
            let out: Vec<Value> = rows
                .iter()
                .map(|r| activity_json(r, source.clone()))
                .collect();
            Ok((StatusCode::OK, Json(Value::Array(out))))
        }
        HistoryBranch::Comments => {
            let rows = fetch_comments(&st.pool, &slug, project_id, issue_id, created_gt).await?;
            let ids: Vec<uuid::Uuid> = rows.iter().map(|r| r.id).collect();
            let reactions = fetch_comment_reactions(&st.pool, &ids).await?;
            let out: Vec<Value> = rows
                .iter()
                .map(|r| {
                    let nested: Vec<Value> = reactions
                        .iter()
                        .filter(|x| x.comment_id == r.id)
                        .map(comment_reaction_json)
                        .collect();
                    comment_json(r, nested)
                })
                .collect();
            Ok((StatusCode::OK, Json(Value::Array(out))))
        }
        HistoryBranch::Merged => {
            let activities =
                fetch_activities(&st.pool, &slug, project_id, issue_id, created_gt).await?;
            let comments =
                fetch_comments(&st.pool, &slug, project_id, issue_id, created_gt).await?;
            let ids: Vec<uuid::Uuid> = comments.iter().map(|r| r.id).collect();
            let reactions = fetch_comment_reactions(&st.pool, &ids).await?;
            // Stable ASC sort over activities-then-comments mirrors
            // `sorted(chain(issue_activities, issue_comments),
            // key=created_at)` (`activity.py:81-84`).
            let mut merged: Vec<(chrono::DateTime<chrono::Utc>, Value)> =
                Vec::with_capacity(activities.len() + comments.len());
            for r in &activities {
                merged.push((r.created_at, activity_json(r, None)));
            }
            for r in &comments {
                let nested: Vec<Value> = reactions
                    .iter()
                    .filter(|x| x.comment_id == r.id)
                    .map(comment_reaction_json)
                    .collect();
                merged.push((r.created_at, comment_json(r, nested)));
            }
            merged.sort_by(|a, b| a.0.cmp(&b.0));
            let out: Vec<Value> = merged.into_iter().map(|(_, v)| v).collect();
            Ok((StatusCode::OK, Json(Value::Array(out))))
        }
    }
}

/// GET `/api/workspaces/:slug/projects/:project_id/issues/:issue_id/meta/`
/// — parity with Django `IssueMetaEndpoint.get`
/// (`plane/app/views/issue/base.py:1186-1198`).
///
/// - Gate: PROJECT ADMIN/MEMBER/GUEST (same shared gate as `history`).
/// - Scope mirrors `Issue.issue_objects` (the LIVE manager,
///   `db/models/issue.py:87-101`): soft-deleted excluded, `archived_at IS
///   NULL`, `is_draft = false`, triage-group states excluded (NULL-state
///   rows KEPT — Django's `exclude(state__group='triage')` is NULL-safe,
///   unlike the Batch C list scope), project not archived/deleted — plus
///   id + project + workspace-slug. Miss → 404 `missing()` (Django
///   `.get()` → `DoesNotExist` → `views/base.py:92-96`).
/// - 200 `{"sequence_id", "project_identifier"}` (key order as Django's
///   `Response` dict).
pub async fn meta(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, issue_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
    )>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let row: Option<(i32, String)> = sqlx::query_as(
        "SELECT i.sequence_id, p.identifier \
        FROM issues i JOIN projects p ON p.id = i.project_id \
        LEFT JOIN states s ON s.id = i.state_id \
        WHERE i.id = $1 AND i.project_id = $2 \
        AND i.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $3) \
        AND i.deleted_at IS NULL AND i.archived_at IS NULL AND i.is_draft = false \
        AND s.\"group\" IS DISTINCT FROM 'triage' \
        AND p.archived_at IS NULL AND p.deleted_at IS NULL",
    )
    .bind(issue_id)
    .bind(project_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some((sequence_id, project_identifier)) => Ok((
            StatusCode::OK,
            Json(json!({"sequence_id": sequence_id, "project_identifier": project_identifier})),
        )),
        None => Ok(missing()),
    }
}

#[cfg(test)]
mod batch_d_d2_tests {
    use super::*;

    #[test]
    fn history_branch_matches_activity_py_switch() {
        // `plane/app/views/issue/activity.py:66-86`: exact
        // `"issue-property"` / `"issue-comment"` take the single-list
        // branches; ANY other value (incl. missing) falls through to the
        // merged default (`activity.py:81-86`).
        assert_eq!(
            history_branch(Some("issue-property")),
            HistoryBranch::Properties
        );
        assert_eq!(
            history_branch(Some("issue-comment")),
            HistoryBranch::Comments
        );
        assert_eq!(history_branch(None), HistoryBranch::Merged);
        assert_eq!(history_branch(Some("junk")), HistoryBranch::Merged);
        assert_eq!(history_branch(Some("")), HistoryBranch::Merged);
    }

    fn sample_activity_row() -> ActivityRow {
        ActivityRow {
            id: uuid::Uuid::nil(),
            created_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            updated_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            created_by_id: Some(uuid::Uuid::nil()),
            updated_by_id: None,
            deleted_at: None,
            project_id: uuid::Uuid::nil(),
            workspace_id: uuid::Uuid::nil(),
            issue_id: Some(uuid::Uuid::nil()),
            verb: "updated".to_string(),
            field: Some("priority".to_string()),
            old_value: Some("low".to_string()),
            new_value: Some("high".to_string()),
            comment: String::new(),
            attachments: vec![],
            issue_comment_id: None,
            actor_id: Some(uuid::Uuid::nil()),
            old_identifier: None,
            new_identifier: None,
            epoch: Some(1.5),
            a_id: Some(uuid::Uuid::nil()),
            a_first_name: Some("Ada".to_string()),
            a_last_name: Some("L".to_string()),
            a_avatar: None,
            a_avatar_asset_id: None,
            a_avatar_entity_type: None,
            a_is_bot: Some(false),
            a_display_name: Some("Ada L".to_string()),
            i_id: Some(uuid::Uuid::nil()),
            i_name: Some("Bug".to_string()),
            i_description_json: Some(json!({})),
            i_description_html: Some("<p></p>".to_string()),
            i_priority: Some("high".to_string()),
            i_start_date: None,
            i_target_date: None,
            i_sequence_id: Some(7),
            i_sort_order: Some(65535.0),
            i_is_draft: Some(false),
            p_id: uuid::Uuid::nil(),
            p_identifier: "WEB".to_string(),
            p_name: "Web".to_string(),
            p_cover_image: None,
            p_cover_asset_id: None,
            p_cover_entity_type: None,
            p_logo_props: json!({}),
            p_description: String::new(),
            w_id: uuid::Uuid::nil(),
            w_name: "Ws".to_string(),
            w_slug: "ws".to_string(),
            w_logo: None,
            w_logo_asset_id: None,
            w_logo_entity_type: None,
        }
    }

    #[test]
    fn activity_json_covers_all_serializer_keys() {
        // Mirrors `IssueActivitySerializer`
        // (`serializers/issue.py:333-351`): model `__all__` (20 columns:
        // id, created_at, updated_at, created_by, updated_by, deleted_at,
        // project, workspace, issue, verb, field, old_value, new_value,
        // comment, attachments, issue_comment, actor, old_identifier,
        // new_identifier, epoch) + actor_detail, issue_detail,
        // project_detail, workspace_detail, source_data.
        let row = sample_activity_row();
        let v = activity_json(
            &row,
            Some(json!({"source": "IN_APP", "source_email": null, "extra": {}})),
        );
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
            "verb",
            "field",
            "old_value",
            "new_value",
            "comment",
            "attachments",
            "issue_comment",
            "actor",
            "old_identifier",
            "new_identifier",
            "epoch",
            "actor_detail",
            "issue_detail",
            "project_detail",
            "workspace_detail",
            "source_data",
        ] {
            assert!(v.get(key).is_some(), "IssueActivity missing key {key}");
        }
        assert_eq!(v.get("verb"), Some(&json!("updated")));
        assert_eq!(
            v.get("source_data"),
            Some(&json!({"source": "IN_APP", "source_email": null, "extra": {}}))
        );
        // No prefetch (merged branch) → `"source_data": null`, mirroring
        // `get_source_data`'s hasattr guards (`serializers/issue.py:340-347`).
        let bare = activity_json(&row, None);
        assert!(bare.get("source_data").unwrap().is_null());
        // Null FKs render null details (nullable `actor`/`issue`).
        let mut null_row = row.clone();
        null_row.a_id = None;
        null_row.i_id = None;
        let nv = activity_json(&null_row, None);
        assert!(nv.get("actor_detail").unwrap().is_null());
        assert!(nv.get("issue_detail").unwrap().is_null());
    }

    #[test]
    fn comment_json_covers_all_serializer_keys_minus_is_member() {
        // Mirrors `IssueCommentSerializer`
        // (`serializers/issue.py:697-707`): model `__all__` (20 columns) +
        // the four `*_detail` objects + `comment_reactions`, and
        // deliberately NO `is_member` — this view annotates none
        // (`activity.py:47-64`, unlike `comment.py:51-57`), so DRF's
        // missing attribute on the non-required `BooleanField` raises
        // `SkipField` and the key is absent on the wire.
        let a = sample_activity_row();
        let row = CommentRow {
            id: a.id,
            created_at: a.created_at,
            updated_at: a.updated_at,
            created_by_id: a.created_by_id,
            updated_by_id: a.updated_by_id,
            deleted_at: a.deleted_at,
            workspace_id: a.workspace_id,
            project_id: a.project_id,
            comment_stripped: Some("hi".to_string()),
            comment_json: json!({}),
            comment_html: "<p>hi</p>".to_string(),
            description_id: None,
            attachments: vec![],
            issue_id: uuid::Uuid::nil(),
            actor_id: a.actor_id,
            access: "INTERNAL".to_string(),
            external_source: None,
            external_id: None,
            edited_at: None,
            parent_id: None,
            a_id: a.a_id,
            a_first_name: a.a_first_name,
            a_last_name: a.a_last_name,
            a_avatar: a.a_avatar,
            a_avatar_asset_id: a.a_avatar_asset_id,
            a_avatar_entity_type: a.a_avatar_entity_type,
            a_is_bot: a.a_is_bot,
            a_display_name: a.a_display_name,
            i_id: a.i_id,
            i_name: a.i_name,
            i_description_json: a.i_description_json,
            i_description_html: a.i_description_html,
            i_priority: a.i_priority,
            i_start_date: a.i_start_date,
            i_target_date: a.i_target_date,
            i_sequence_id: a.i_sequence_id,
            i_sort_order: a.i_sort_order,
            i_is_draft: a.i_is_draft,
            p_id: a.p_id,
            p_identifier: a.p_identifier,
            p_name: a.p_name,
            p_cover_image: a.p_cover_image,
            p_cover_asset_id: a.p_cover_asset_id,
            p_cover_entity_type: a.p_cover_entity_type,
            p_logo_props: a.p_logo_props,
            p_description: a.p_description,
            w_id: a.w_id,
            w_name: a.w_name,
            w_slug: a.w_slug,
            w_logo: a.w_logo,
            w_logo_asset_id: a.w_logo_asset_id,
            w_logo_entity_type: a.w_logo_entity_type,
        };
        let v = comment_json(&row, vec![]);
        for key in [
            "id",
            "created_at",
            "updated_at",
            "created_by",
            "updated_by",
            "deleted_at",
            "workspace",
            "project",
            "comment_stripped",
            "comment_json",
            "comment_html",
            "description",
            "attachments",
            "issue",
            "actor",
            "access",
            "external_source",
            "external_id",
            "edited_at",
            "parent",
            "actor_detail",
            "issue_detail",
            "project_detail",
            "workspace_detail",
            "comment_reactions",
        ] {
            assert!(v.get(key).is_some(), "IssueComment missing key {key}");
        }
        assert!(
            v.get("is_member").is_none(),
            "is_member must be absent (SkipField)"
        );
        assert_eq!(v.get("comment_reactions"), Some(&Value::Array(vec![])));
    }

    #[test]
    fn comment_reaction_json_covers_all_serializer_keys() {
        // Mirrors `CommentReactionSerializer`
        // (`serializers/issue.py:666-685`): id, actor, comment, reaction,
        // display_name, deleted_at, workspace, project, created_at,
        // updated_at, created_by, updated_by.
        let row = CommentReactionRow {
            id: uuid::Uuid::nil(),
            actor_id: Some(uuid::Uuid::nil()),
            comment_id: uuid::Uuid::nil(),
            reaction: "heart".to_string(),
            actor_display_name: Some("Ada L".to_string()),
            deleted_at: None,
            workspace_id: uuid::Uuid::nil(),
            project_id: uuid::Uuid::nil(),
            created_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            updated_at: "2026-01-02T03:04:05Z".parse().unwrap(),
            created_by_id: None,
            updated_by_id: None,
        };
        let v = comment_reaction_json(&row);
        for key in [
            "id",
            "actor",
            "comment",
            "reaction",
            "display_name",
            "deleted_at",
            "workspace",
            "project",
            "created_at",
            "updated_at",
            "created_by",
            "updated_by",
        ] {
            assert!(v.get(key).is_some(), "CommentReaction missing key {key}");
        }
        assert_eq!(v.get("display_name"), Some(&json!("Ada L")));
    }

    #[test]
    fn lite_detail_helpers_render_null_when_fk_null() {
        assert!(actor_detail_json(None, &None, &None, &None, None, None, &None, &None).is_null());
        assert!(
            issue_flat_json(None, &None, None, &None, &None, &None, &None, None, None, None)
                .is_null()
        );
        let ws = workspace_lite_json(uuid::Uuid::nil(), "Ws", "ws", &None, None, None);
        assert_eq!(ws.get("slug"), Some(&json!("ws")));
        let proj = project_lite_json(
            uuid::Uuid::nil(),
            "WEB",
            "Web",
            &None,
            None,
            None,
            &json!({}),
            "",
        );
        assert_eq!(proj.get("identifier"), Some(&json!("WEB")));
    }
}
