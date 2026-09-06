//! Workspace + project members — parity with Django
//! `plane/app/views/workspace/member.py:45-266` and
//! `plane/app/views/project/member.py:46-408`.
//!
//! Celery/email side-effects are SKIPPED (rows created, success messages kept).
//! Django crashes (`.get()` misses on non-decorated paths, `int()` on garbage,
//! `None` role reactivation) return sane 4xx here — each documented at the call
//! site. Soft-delete rule: members deactivate via `is_active = false`
//! (never `UPDATE deleted_at`); invite rows hard-delete — see `invite.rs`.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::routes::issue_common::{
    fetch_project_member_role, is_workspace_admin, project_gate_allows,
};
use crate::routes::project::{deny, guard_leave, missing, ws_role};
use crate::{middleware::auth::AuthUser, state::AppState};

// ============================================================================
// Error strings — every literal quoted from Django with file:line.
// ============================================================================

/// `plane/app/views/base.py:92-97` (DRF `ValidationError` → 400).
pub const VALID_DETAIL_MSG: &str = "Please provide valid detail";
/// DRF default permission-class deny body (e.g. `WorkspaceEntityPermission`,
/// `WorkSpaceAdminPermission` denials — NOT the `allow_permission` body).
pub const PERMISSION_DETAIL_MSG: &str = "You do not have permission to perform this action.";
/// `plane/app/views/workspace/member.py:66` (retrieve miss — verbatim).
pub const WS_MEMBER_NOT_FOUND_MSG: &str = "Workspace member not found";
/// `plane/app/views/workspace/member.py:83` and
/// `plane/app/views/project/member.py:222` (self role update — same text).
pub const SELF_ROLE_UPDATE_MSG: &str = "You cannot update your own role";
/// `plane/app/views/workspace/member.py:112` (ws destroy self) and
/// `plane/app/views/project/member.py:309` (project destroy self — sic
/// "workspace" reused verbatim on the project path).
pub const SELF_REMOVE_MSG: &str = "You cannot remove yourself from the workspace. Please use leave workspace";
/// `plane/app/views/workspace/member.py:118` and
/// `plane/app/views/project/member.py:315` (same text both paths).
pub const HIGHER_THAN_YOU_MSG: &str = "You cannot remove a user having role higher than you";
/// `plane/app/views/workspace/member.py:138` (ws destroy sole-project-admin —
/// trailing period is verbatim).
pub const SOLE_PROJ_ADMIN_USER_MSG: &str = "User is a part of some projects where they are the only admin, they should either leave that project or promote another user to admin.";
/// `plane/app/views/workspace/member.py:171` (ws leave sole-ws-admin).
pub const SOLE_WS_ADMIN_LEAVE_MSG: &str = "You cannot leave the workspace as you are the only admin of the workspace you will have to either delete the workspace or promote another user to admin.";
/// `plane/app/views/workspace/member.py:193` (ws leave sole-project-admin).
pub const SOLE_PROJ_ADMIN_LEAVE_MSG: &str = "You are a part of some projects where you are the only admin, you should either leave the project or promote another user to admin.";
/// `plane/app/views/project/member.py:194` (project retrieve miss — verbatim).
pub const PROJ_MEMBER_NOT_FOUND_MSG: &str = "Project member not found";
/// `plane/app/views/project/member.py:57` (bulk create empty).
pub const MEMBERS_REQUIRED_MSG: &str = "At least one member is required";
/// `plane/app/views/project/member.py:75` (bulk role below ws role).
pub const WS_ROLE_LOWER_MSG: &str = "You cannot add a user with role lower than the workspace role";
/// `plane/app/views/project/member.py:81` (bulk role above ws role).
pub const WS_ROLE_HIGHER_MSG: &str = "You cannot add a user with role higher than the workspace role";
/// `plane/app/views/project/member.py:237` (non-admin role update — 403).
pub const ROLE_UPDATE_FORBIDDEN_MSG: &str = "You do not have permission to update roles";
/// `plane/app/views/project/member.py:244` (target >= requester — 403).
pub const TARGET_GTE_MSG: &str =
    "You cannot update the role of a member with a role equal to or higher than your own";
/// `plane/app/views/project/member.py:253` (new >= requester — 403).
pub const NEW_GTE_MSG: &str = "You cannot assign a role equal to or higher than your own";
/// `plane/app/views/project/member.py:273` (non-admin `is_active` flip — 403).
pub const STATUS_UPDATE_FORBIDDEN_MSG: &str = "You do not have permission to update member status";
/// `plane/app/views/project/member.py:279` (target >= requester — 403).
pub const TARGET_STATUS_GTE_MSG: &str =
    "You cannot update the status of a member with a role equal to or higher than your own";

/// DRF permission-class deny body — same shape as the E2 `cycle::deny_detail`
/// / E3 `module::deny_detail` helper.
pub(crate) fn deny_detail() -> (StatusCode, Json<Value>) {
    (
        StatusCode::FORBIDDEN,
        Json(json!({"detail": PERMISSION_DETAIL_MSG})),
    )
}

// ============================================================================
// Legacy pure surface (kept for `crates/api/tests/member_test.rs` +
// `detail_member_test.rs`; CONSTRAINTS forbid touching those files).
// ============================================================================

/// Mirrors `plane/api/serializers/member.py:ProjectMemberSerializer`
/// (pre-E5 construction point for tests).
/// `#[allow(dead_code)]`: the Axum handlers take `Json<Value>` bodies
/// (Django reads `request.data` dynamically), so this typed helper is a
/// construction point for tests only (E2 `cycle.rs:CreateCycle` precedent).
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct CreateMember {
    #[serde(default)]
    pub member: Option<uuid::Uuid>,
    #[serde(default)]
    pub role: Option<i16>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub struct MemberOut {
    pub id: uuid::Uuid,
    pub member: Option<uuid::Uuid>,
    pub role: i16,
}

/// Mirrors `plane/api/serializers/invite.py:WorkspaceInviteSerializer`
/// (pre-E5 construction point for tests; the E5 invite handlers live in
/// `invite.rs` and reuse `is_valid_email` below).
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct CreateInvite {
    pub email: String,
    #[serde(default)]
    pub role: Option<i16>,
}

pub const ROLES: [i16; 3] = [20, 15, 5];

#[allow(dead_code)]
pub fn validate_create(body: &CreateMember) -> Result<(), String> {
    match body.member {
        Some(_) => {}
        None => return Err("Member is required".to_string()),
    }
    let role = body.role.unwrap_or(5);
    if !ROLES.contains(&role) {
        return Err("Invalid role".to_string());
    }
    Ok(())
}

pub fn is_valid_email(email: &str) -> bool {
    let mut parts = email.split('@');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(local), Some(domain), None) => {
            !local.is_empty() && domain.contains('.') && !domain.starts_with('.')
        }
        _ => false,
    }
}

#[allow(dead_code)]
pub fn validate_invite_create(body: &CreateInvite) -> Result<(), String> {
    if !is_valid_email(&body.email) {
        return Err("Invalid email address".to_string());
    }
    let role = body.role.unwrap_or(5);
    if !ROLES.contains(&role) {
        return Err("Invalid role".to_string());
    }
    Ok(())
}

/// Mirrors `plane/app/views/project/member.py:partial_update` self branch
/// (and the workspace twin): a non-admin cannot change their own role.
/// `#[allow(dead_code)]`: legacy test-only surface (see `CreateMember`
/// above); handlers use the `*_MSG` consts directly.
#[allow(dead_code)]
pub fn guard_patch_self(is_self: bool, is_admin: bool) -> Result<(), String> {
    if is_self && !is_admin {
        return Err(SELF_ROLE_UPDATE_MSG.to_string());
    }
    Ok(())
}

/// Mirrors `destroy`: self-removal must go through leave-workspace.
#[allow(dead_code)]
pub fn guard_destroy_self(is_self: bool) -> Result<(), String> {
    if is_self {
        return Err(SELF_REMOVE_MSG.to_string());
    }
    Ok(())
}

/// Mirrors `destroy`: requester role must not be lower than the target's.
pub fn guard_destroy_role(requester_role: i16, target_role: i16) -> Result<(), String> {
    if requester_role < target_role {
        return Err(HIGHER_THAN_YOU_MSG.to_string());
    }
    Ok(())
}

// ============================================================================
// Pure helpers (unit-tested below).
// ============================================================================

/// Mirrors the admin-vs-lite serializer switch
/// (`plane/app/views/workspace/member.py:51,70` and
/// `plane/app/views/project/member.py:198`): the admin (email-bearing)
/// shape renders iff the requester role is ABOVE guest (`> 5`).
pub fn sees_admin_shape(requester_role: i16) -> bool {
    requester_role > 5
}

/// Outcome of the project PATCH gate ladder
/// (`plane/app/views/project/member.py:206-288`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchDeny {
    BadRequest(&'static str),
    Forbidden(&'static str),
}

/// Context for [`project_patch_decision`]: roles are i16, `new_role` is
/// `Some` iff `"role"` is in the body, `touches_active` iff `"is_active"`
/// is in the body.
pub struct PatchGateCtx {
    pub is_self: bool,
    pub is_ws_admin: bool,
    pub requester_role: i16,
    pub target_role: i16,
    pub new_role: Option<i16>,
    pub target_ws_role: i16,
    pub touches_active: bool,
}

/// Mirrors the FULL `partial_update` gate ladder in Django order
/// (`plane/app/views/project/member.py:219-281`):
/// 1. self-update by non-ws-admin → 400 (`:220-224`);
/// 2. with `"role"`: non-admin non-ws-admin → 403 (`:235-239`);
///    target ≥ requester (non-ws-admin) → 403 (`:242-246`);
///    new ≥ requester (non-ws-admin) → 403 (`:251-255`);
///    ws-guest capped above member (non-ws-admin bypasses NOT this check —
///    `:258-262` runs for ws-admins too) → 400;
/// 3. with `"is_active"`: non-admin non-ws-admin → 403 (`:271-275`);
///    target ≥ requester (non-ws-admin) → 403 (`:277-281`).
pub fn project_patch_decision(ctx: &PatchGateCtx) -> Result<(), PatchDeny> {
    // `member.py:220-224` — self role update.
    if ctx.is_self && !ctx.is_ws_admin {
        return Err(PatchDeny::BadRequest(SELF_ROLE_UPDATE_MSG));
    }
    // `member.py:233-262` — role block.
    if let Some(new_role) = ctx.new_role {
        // `member.py:235-239` — only admins may touch roles.
        if ctx.requester_role < 20 && !ctx.is_ws_admin {
            return Err(PatchDeny::Forbidden(ROLE_UPDATE_FORBIDDEN_MSG));
        }
        // `member.py:242-246` — cannot touch target ≥ self.
        if ctx.target_role >= ctx.requester_role && !ctx.is_ws_admin {
            return Err(PatchDeny::Forbidden(TARGET_GTE_MSG));
        }
        // `member.py:251-255` — cannot grant ≥ self.
        if new_role >= ctx.requester_role && !ctx.is_ws_admin {
            return Err(PatchDeny::Forbidden(NEW_GTE_MSG));
        }
        // `member.py:258-262` — ws-guest cap (no ws-admin bypass here).
        if ctx.target_ws_role == 5 && (new_role == 15 || new_role == 20) {
            return Err(PatchDeny::BadRequest(WS_ROLE_HIGHER_MSG));
        }
    }
    // `member.py:270-281` — `is_active` block.
    if ctx.touches_active {
        // `member.py:271-275`.
        if ctx.requester_role < 20 && !ctx.is_ws_admin {
            return Err(PatchDeny::Forbidden(STATUS_UPDATE_FORBIDDEN_MSG));
        }
        // `member.py:277-281`.
        if ctx.target_role >= ctx.requester_role && !ctx.is_ws_admin {
            return Err(PatchDeny::Forbidden(TARGET_STATUS_GTE_MSG));
        }
    }
    Ok(())
}

/// One bulk-create entry (`plane/app/views/project/member.py:49,66`):
/// `member_id` + optional role (Django `member.get("role")` defaults to 5
/// at insert, `member.py:118`).
#[derive(Debug, Clone)]
pub struct BulkEntry {
    pub member_id: uuid::Uuid,
    pub role: Option<i16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MembersParseError {
    /// Missing / null / non-array / empty → 400
    /// `{"error":"At least one member is required"}` (`member.py:55-59`).
    /// (Django `request.data.get("members", [])` + `len()` check; a non-list
    /// body would 500 there — sane 400 here, documented.)
    Empty,
    /// Unparseable `member_id` → 400 valid-detail (Django
    /// `ValidationError` → `views/base.py:92-97`). A JSON null id falls
    /// through to the 404 path instead (Django `.get(member=None)` →
    /// `DoesNotExist` → 404) — signalled via `NullId`.
    Invalid,
    NullId,
}

/// Parses the `{members:[{member_id,role?}]}` bulk body
/// (`plane/app/views/project/member.py:46-66`).
pub fn parse_members_body(body: &Value) -> Result<Vec<BulkEntry>, MembersParseError> {
    let arr = match body.get("members") {
        Some(Value::Array(a)) => a,
        _ => return Err(MembersParseError::Empty),
    };
    if arr.is_empty() {
        return Err(MembersParseError::Empty);
    }
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let member_id = match item.get("member_id") {
            None | Some(Value::Null) => return Err(MembersParseError::NullId),
            Some(Value::String(s)) => s.parse().map_err(|_| MembersParseError::Invalid)?,
            Some(_) => return Err(MembersParseError::Invalid),
        };
        let role = match item.get("role") {
            None | Some(Value::Null) => None,
            Some(v) => Some(coerce_role_value(v).map_err(|_| MembersParseError::Invalid)?),
        };
        out.push(BulkEntry { member_id, role });
    }
    Ok(out)
}

/// Coerces a JSON role to i16 (numbers + integer strings — Django
/// `int(...)` accepts both, `member.py:88,248`). Booleans/objects/arrays
/// → Err (Django would 500 on `int()` — sane 400, documented).
pub fn coerce_role_value(v: &Value) -> Result<i16, ()> {
    match v {
        Value::Number(n) => n.as_i64().and_then(|i| i16::try_from(i).ok()).ok_or(()),
        Value::String(s) => s.trim().parse::<i16>().map_err(|_| ()),
        _ => Err(()),
    }
}

/// DRF `ChoiceField` failure shape for role-ish fields
/// (`"99" is not a valid choice.`).
pub fn invalid_choice_msg(raw: &str) -> String {
    format!("\"{raw}\" is not a valid choice.")
}

/// Merges a PATCH preferences object into stored preferences, mirroring
/// `ProjectMemberPreferenceSerializer.validate_preferences`
/// (`plane/app/serializers/project.py:166-175` — `dict.update`, shallow).
/// Non-object patch → Err (Django `AttributeError` → 500 — sane 400,
/// documented). Non-object stored (legacy rows) → starts from `{}`.
pub fn merge_preferences(stored: &Value, patch: &Value) -> Result<Value, ()> {
    let obj = patch.as_object().ok_or(())?;
    let mut merged = stored.as_object().cloned().unwrap_or_default();
    for (k, v) in obj {
        merged.insert(k.clone(), v.clone());
    }
    Ok(Value::Object(merged))
}

// ============================================================================
// Shared gates + lookups.
// ============================================================================

/// Workspace-level AMG gate mirroring
/// `@allow_permission([ADMIN, MEMBER, GUEST], level="WORKSPACE")`
/// (`plane/app/permissions/base.py:46-51`): any ACTIVE ws membership with
/// role 20/15/5. Deny is `deny()` 403 `{"error": ...}`.
async fn gate_ws_amg(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
) -> Result<Option<i16>, sqlx::Error> {
    let role = ws_role(pool, user, slug).await?;
    Ok(match role {
        Some(20) | Some(15) | Some(5) => role,
        _ => None,
    })
}

/// Workspace-level ADMIN gate mirroring
/// `@allow_permission([ADMIN], level="WORKSPACE")`
/// (`workspace/member.py:76,98`): ACTIVE role-20 membership.
async fn gate_ws_admin(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
) -> Result<bool, sqlx::Error> {
    Ok(ws_role(pool, user, slug).await?.is_some_and(|r| r == 20))
}

/// Project-level gate mirroring `@allow_permission(..., level="PROJECT")`
/// (`plane/app/permissions/base.py:53-78`): allowed role outright, else the
/// workspace-ADMIN fallback (any project membership + ws admin).
pub(crate) async fn gate_project(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
    admin_only: bool,
) -> Result<Option<i16>, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, pid).await?;
    let allowed = if admin_only {
        role == Some(20)
    } else {
        matches!(role, Some(20) | Some(15) | Some(5))
    };
    if allowed {
        return Ok(role);
    }
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    if project_gate_allows(false, role.is_some(), ws_admin) {
        return Ok(role);
    }
    Ok(None)
}

/// Project id + workspace id for slug-scoped project endpoints; None when
/// the project is not in the workspace (callers answer `missing()`).
pub(crate) async fn project_in_workspace(
    pool: &sqlx::PgPool,
    pid: uuid::Uuid,
    slug: &str,
) -> Result<Option<uuid::Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT p.workspace_id FROM projects p JOIN workspaces w ON w.id = p.workspace_id \
         WHERE p.id = $1 AND w.slug = $2 AND p.deleted_at IS NULL",
    )
    .bind(pid)
    .bind(slug)
    .fetch_optional(pool)
    .await
}

/// Sole-project-admin probe mirroring the annotate in
/// `workspace/member.py:122-135` (destroy) and `:176-189` (leave):
/// some project in the workspace whose non-deleted members are exactly one
/// role-20 row for `user_id`.
///
/// Deviation (documented): Django `:128` compares
/// `project_projectmember__member_id=workspace_member.id` — the
/// WorkspaceMember ROW id, not the user id — so the destroy check can only
/// fire on UUID coincidence (dead code). The leave twin (`:182`) correctly
/// uses `request.user.id`. Rust uses the user id on both paths (intended
/// behavior; the quoted 400 is preserved verbatim).
async fn sole_project_admin_exists(
    pool: &sqlx::PgPool,
    workspace_id: uuid::Uuid,
    user_id: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM projects p WHERE p.workspace_id = $1 AND p.deleted_at IS NULL \
         AND (SELECT COUNT(*) FROM project_members pm WHERE pm.project_id = p.id AND pm.deleted_at IS NULL) = 1 \
         AND (SELECT COUNT(*) FROM project_members pm WHERE pm.project_id = p.id \
              AND pm.member_id = $2 AND pm.role = 20 AND pm.deleted_at IS NULL) = 1)",
    )
    .bind(workspace_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
}

fn opt_uuid(u: &Option<uuid::Uuid>) -> Value {
    u.map(|v| json!(v)).unwrap_or(Value::Null)
}

// ============================================================================
// Row structs + JSON builders.
// ============================================================================

#[derive(Debug, Clone, sqlx::FromRow)]
struct WsMemberRow {
    id: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    workspace_id: uuid::Uuid,
    member_id: uuid::Uuid,
    role: i16,
    company_role: Option<String>,
    view_props: Value,
    default_props: Value,
    issue_props: Value,
    is_active: bool,
    getting_started_checklist: Value,
    tips: Value,
    explored_features: Value,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
    u_first_name: String,
    u_last_name: String,
    u_avatar: String,
    u_avatar_url: String,
    u_is_bot: bool,
    u_display_name: String,
    u_email: Option<String>,
    u_last_login_medium: String,
}

const WS_MEMBER_COLS: &str = "wm.id, wm.created_at, wm.updated_at, wm.workspace_id, wm.member_id, \
    wm.role, wm.company_role, wm.view_props, wm.default_props, wm.issue_props, wm.is_active, \
    wm.getting_started_checklist, wm.tips, wm.explored_features, \
    wm.created_by_id, wm.updated_by_id, \
    u.first_name AS u_first_name, u.last_name AS u_last_name, u.avatar AS u_avatar, \
    CASE WHEN u.avatar_asset_id IS NOT NULL \
      THEN '/api/assets/v2/static/' || u.avatar_asset_id::text || '/' ELSE u.avatar END AS u_avatar_url, \
    u.is_bot AS u_is_bot, u.display_name AS u_display_name, u.email AS u_email, \
    u.last_login_medium AS u_last_login_medium";

fn lite_user_json(r: &WsMemberRow) -> Value {
    json!({
        "id": r.member_id,
        "first_name": r.u_first_name,
        "last_name": r.u_last_name,
        "avatar": r.u_avatar,
        "avatar_url": r.u_avatar_url,
        "is_bot": r.u_is_bot,
        "display_name": r.u_display_name,
    })
}

fn admin_user_json(r: &WsMemberRow) -> Value {
    json!({
        "id": r.member_id,
        "first_name": r.u_first_name,
        "last_name": r.u_last_name,
        "avatar": r.u_avatar,
        "avatar_url": r.u_avatar_url,
        "is_bot": r.u_is_bot,
        "display_name": r.u_display_name,
        "email": r.u_email,
        "last_login_medium": r.u_last_login_medium,
    })
}

/// List/retrieve projection (`fields=("id","member","role")`,
/// `workspace/member.py:52,54,71,73`).
fn ws_member_short_json(r: &WsMemberRow, admin: bool) -> Value {
    json!({
        "id": r.id,
        "member": if admin { admin_user_json(r) } else { lite_user_json(r) },
        "role": r.role,
    })
}

/// Full `WorkSpaceMemberSerializer` shape (`fields="__all__"`,
/// `serializers/workspace.py:93-99`, member nested lite) — the PATCH
/// response (`workspace/member.py:91-95`).
fn ws_member_full_json(r: &WsMemberRow) -> Value {
    json!({
        "id": r.id,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "workspace": r.workspace_id,
        "member": lite_user_json(r),
        "role": r.role,
        "company_role": r.company_role,
        "view_props": r.view_props,
        "default_props": r.default_props,
        "issue_props": r.issue_props,
        "is_active": r.is_active,
        "getting_started_checklist": r.getting_started_checklist,
        "tips": r.tips,
        "explored_features": r.explored_features,
        "created_by": opt_uuid(&r.created_by_id),
        "updated_by": opt_uuid(&r.updated_by_id),
    })
}

/// Full `WorkspaceMemberMeSerializer` shape (`fields="__all__"` + annotated
/// `draft_issue_count`, `workspace/member.py:220-234`): FKs render as PKs
/// (no nested serializer declared). Non-member → 200 null (Django
/// serializes `.first()` → None as null — preserved).
fn ws_me_json(r: &WsMemberRow, draft_issue_count: i64) -> Value {
    let mut v = ws_member_full_json(r);
    let obj = v.as_object_mut().expect("ws member json is object");
    obj.insert("member".to_string(), json!(r.member_id));
    obj.insert("draft_issue_count".to_string(), json!(draft_issue_count));
    v
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PmShortRow {
    id: uuid::Uuid,
    member_id: Option<uuid::Uuid>,
    role: i16,
    project_id: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// `ProjectMemberRoleSerializer` (`serializers/project.py:190-197`):
/// `(id, role, member, project, original_role, created_at)`.
fn pm_role_json(r: &PmShortRow) -> Value {
    json!({
        "id": r.id,
        "role": r.role,
        "member": opt_uuid(&r.member_id),
        "project": r.project_id,
        "original_role": r.role,
        "created_at": r.created_at,
    })
}

/// List projection (`fields=("id","member","role")`,
/// `project/member.py:168`).
fn pm_role_short_json(r: &PmShortRow) -> Value {
    json!({
        "id": r.id,
        "member": opt_uuid(&r.member_id),
        "role": r.role,
    })
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PmFullRow {
    id: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    workspace_id: uuid::Uuid,
    project_id: uuid::Uuid,
    member_id: Option<uuid::Uuid>,
    comment: Option<String>,
    role: i16,
    view_props: Value,
    default_props: Value,
    preferences: Value,
    sort_order: f64,
    is_active: bool,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
    ws_name: String,
    ws_slug: String,
    ws_logo: Option<String>,
    proj_identifier: String,
    proj_name: String,
    proj_cover_image: Option<String>,
    proj_logo_props: Value,
    proj_description: String,
    u_first_name: Option<String>,
    u_last_name: Option<String>,
    u_avatar: Option<String>,
    u_avatar_url: Option<String>,
    u_is_bot: Option<bool>,
    u_display_name: Option<String>,
    u_email: Option<String>,
    u_last_login_medium: Option<String>,
}

const PM_FULL_COLS: &str = "pm.id, pm.created_at, pm.updated_at, pm.workspace_id, pm.project_id, \
    pm.member_id, pm.comment, pm.role, pm.view_props, pm.default_props, pm.preferences, \
    pm.sort_order, pm.is_active, pm.created_by_id, pm.updated_by_id, \
    w.name AS ws_name, w.slug AS ws_slug, w.logo AS ws_logo, \
    p.identifier AS proj_identifier, p.name AS proj_name, p.cover_image AS proj_cover_image, \
    p.logo_props AS proj_logo_props, p.description AS proj_description, \
    u.first_name AS u_first_name, u.last_name AS u_last_name, u.avatar AS u_avatar, \
    CASE WHEN u.avatar_asset_id IS NOT NULL \
      THEN '/api/assets/v2/static/' || u.avatar_asset_id::text || '/' ELSE u.avatar END AS u_avatar_url, \
    u.is_bot AS u_is_bot, u.display_name AS u_display_name, u.email AS u_email, \
    u.last_login_medium AS u_last_login_medium";

/// `logo_url` mirrors `Workspace.logo_url` asset-first fallback
/// (`db/models/workspace.py:146-154`); asset-backed logos are simplified to
/// the `logo` column here — same precedent as `users_me::pick_logo_url`
/// (invite/logo shapes never join `file_assets`).
fn pm_member_json(r: &PmFullRow, admin: bool) -> Value {
    let Some(mid) = r.member_id else {
        return Value::Null;
    };
    let mut m = serde_json::Map::new();
    m.insert("id".to_string(), json!(mid));
    m.insert("first_name".to_string(), json!(r.u_first_name));
    m.insert("last_name".to_string(), json!(r.u_last_name));
    m.insert("avatar".to_string(), json!(r.u_avatar));
    m.insert("avatar_url".to_string(), json!(r.u_avatar_url));
    m.insert("is_bot".to_string(), json!(r.u_is_bot));
    m.insert("display_name".to_string(), json!(r.u_display_name));
    if admin {
        m.insert("email".to_string(), json!(r.u_email));
        m.insert(
            "last_login_medium".to_string(),
            json!(r.u_last_login_medium),
        );
    }
    Value::Object(m)
}

fn pm_full_json(r: &PmFullRow, admin: bool) -> Value {
    json!({
        "id": r.id,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "workspace": {"name": r.ws_name, "slug": r.ws_slug, "id": r.workspace_id, "logo_url": r.ws_logo},
        "project": {"id": r.project_id, "identifier": r.proj_identifier, "name": r.proj_name,
            "cover_image": r.proj_cover_image, "cover_image_url": r.proj_cover_image,
            "logo_props": r.proj_logo_props, "description": r.proj_description},
        "member": pm_member_json(r, admin),
        "comment": r.comment,
        "role": r.role,
        "view_props": r.view_props,
        "default_props": r.default_props,
        "preferences": r.preferences,
        "sort_order": r.sort_order,
        "is_active": r.is_active,
        "created_by": opt_uuid(&r.created_by_id),
        "updated_by": opt_uuid(&r.updated_by_id),
    })
}

// ============================================================================
// E5a — workspace members.
// ============================================================================

/// Mirrors `WorkSpaceMemberViewSet.list`
/// (`plane/app/views/workspace/member.py:45-55`): GET 200
/// `[{id, member, role}]`; the admin (email-bearing) member shape renders
/// iff the REQUESTER role is > 5 (`:51`). No `is_active` filter on the
/// queryset (`:37-43` — preserved literally; PATCH/DELETE add it).
pub async fn list_workspace_members(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let Some(requester_role) = gate_ws_amg(&st.pool, auth.0, &slug).await? else {
        return Ok(deny());
    };
    let admin = sees_admin_shape(requester_role);
    let rows: Vec<WsMemberRow> = sqlx::query_as(&format!(
        "SELECT {WS_MEMBER_COLS} FROM workspace_members wm \
         JOIN workspaces w ON w.id = wm.workspace_id \
         JOIN users u ON u.id = wm.member_id \
         WHERE w.slug = $1 AND wm.deleted_at IS NULL ORDER BY wm.created_at DESC"
    ))
    .bind(&slug)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!(rows.iter().map(|r| ws_member_short_json(r, admin)).collect::<Vec<_>>())),
    ))
}

/// Mirrors `WorkSpaceMemberViewSet.retrieve`
/// (`plane/app/views/workspace/member.py:57-74`): GET 200; miss → 404
/// `{"error":"Workspace member not found"}` verbatim (`:64-68`).
pub async fn ws_member_detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let Some(requester_role) = gate_ws_amg(&st.pool, auth.0, &slug).await? else {
        return Ok(deny());
    };
    let row: Option<WsMemberRow> = sqlx::query_as(&format!(
        "SELECT {WS_MEMBER_COLS} FROM workspace_members wm \
         JOIN workspaces w ON w.id = wm.workspace_id \
         JOIN users u ON u.id = wm.member_id \
         WHERE w.slug = $1 AND wm.id = $2 AND wm.deleted_at IS NULL"
    ))
    .bind(&slug)
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    let Some(row) = row else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": WS_MEMBER_NOT_FOUND_MSG})),
        ));
    };
    Ok((
        StatusCode::OK,
        Json(ws_member_short_json(&row, sees_admin_shape(requester_role))),
    ))
}

/// Mirrors `WorkSpaceMemberViewSet.partial_update`
/// (`plane/app/views/workspace/member.py:76-96`): gate ADMIN; self → 400
/// (`:81-85`); `role == 5` cascades `project_members.role = 5` (`:88-89`);
/// serializer errors → 400; 200 full lite shape.
pub async fn ws_member_patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_admin(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    // Target: pk + slug + non-bot + active (`:78-80`). Miss → Django
    // `.get()` → `DoesNotExist` → 404 `missing()` (the custom message is
    // retrieve-only, `:64-68`).
    let target: Option<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT wm.member_id FROM workspace_members wm \
         JOIN workspaces w ON w.id = wm.workspace_id JOIN users u ON u.id = wm.member_id \
         WHERE wm.id = $1 AND w.slug = $2 AND u.is_bot = false \
         AND wm.is_active = true AND wm.deleted_at IS NULL",
    )
    .bind(pk)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some((target_user,)) = target else {
        return Ok(missing());
    };
    // `member.py:81-85` — self role update.
    if target_user == auth.0 {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": SELF_ROLE_UPDATE_MSG})),
        ));
    }
    // Validate `role` like the `ChoiceField` (`ROLE_CHOICES` 20/15/5):
    // missing → no-op; garbage → 400 serializer-errors shape.
    let new_role: Option<i16> = match body.get("role") {
        None | Some(Value::Null) => None,
        Some(Value::Number(n)) => match n.as_i64().and_then(|i| i16::try_from(i).ok()) {
            Some(r) => Some(r),
            None => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"role": [invalid_choice_msg(&n.to_string())]})),
                ));
            }
        },
        Some(Value::String(s)) => match s.trim().parse::<i16>() {
            Ok(r) => Some(r),
            Err(_) => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"role": [invalid_choice_msg(s.trim())]})),
                ));
            }
        },
        Some(_) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": VALID_DETAIL_MSG})),
            ));
        }
    };
    if let Some(r) = new_role {
        if !ROLES.contains(&r) {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"role": [invalid_choice_msg(&r.to_string())]})),
            ));
        }
    }
    // `company_role` is a nullable text field (`db/models/workspace.py`);
    // non-string → DRF `Not a valid string.`
    let company_role: Option<Option<String>> = match body.get("company_role") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(s)) => Some(Some(s.clone())),
        Some(_) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"company_role": ["Not a valid string."]})),
            ));
        }
    };
    // Unknown keys are ignored (DRF silently drops undeclared input keys).
    let mut tx = st.pool.begin().await?;
    // `member.py:88-89` — guest demotion cascades to project roles.
    if new_role == Some(5) {
        sqlx::query(
            "UPDATE project_members pm SET role = 5, updated_at = now() FROM workspaces w \
             WHERE pm.workspace_id = w.id AND w.slug = $1 AND pm.member_id = $2 \
             AND pm.deleted_at IS NULL",
        )
        .bind(&slug)
        .bind(target_user)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "UPDATE workspace_members wm SET role = COALESCE($1, wm.role), \
         company_role = COALESCE($2, wm.company_role), updated_at = now() \
         FROM workspaces w WHERE wm.id = $3 AND wm.workspace_id = w.id AND w.slug = $4",
    )
    .bind(new_role)
    .bind(company_role.clone().unwrap_or(None))
    .bind(pk)
    .bind(&slug)
    .execute(&mut *tx)
    .await?;
    // NOTE: `COALESCE($2, ...)` cannot express explicit-null company_role
    // clear; Django applies explicit null. Explicit-null clear is a
    // documented micro-deviation (role path — the quoted contract — is exact).
    if company_role == Some(None) {
        sqlx::query(
            "UPDATE workspace_members wm SET company_role = NULL, updated_at = now() \
             FROM workspaces w WHERE wm.id = $1 AND wm.workspace_id = w.id AND w.slug = $2",
        )
        .bind(pk)
        .bind(&slug)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    let row: Option<WsMemberRow> = sqlx::query_as(&format!(
        "SELECT {WS_MEMBER_COLS} FROM workspace_members wm \
         JOIN workspaces w ON w.id = wm.workspace_id \
         JOIN users u ON u.id = wm.member_id \
         WHERE w.slug = $1 AND wm.id = $2 AND wm.deleted_at IS NULL"
    ))
    .bind(&slug)
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(r) => Ok((StatusCode::OK, Json(ws_member_full_json(&r)))),
        None => Ok(missing()),
    }
}

/// Mirrors `WorkSpaceMemberViewSet.destroy`
/// (`plane/app/views/workspace/member.py:98-150`): gate ADMIN; self → 400
/// (`:110-114`); requester < target → 400 (`:116-120`); sole-project-admin
/// target → 400 verbatim (`:122-141`); else SOFT-deactivate the ws row +
/// all project rows (`is_active = false`, `:144-149` — never
/// `UPDATE deleted_at`) → **204**. Multi-write in one tx.
pub async fn ws_member_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_admin(&st.pool, auth.0, &slug).await? {
        return Ok(deny());
    }
    let target: Option<(uuid::Uuid, uuid::Uuid, i16)> = sqlx::query_as(
        "SELECT wm.id, wm.member_id, wm.role FROM workspace_members wm \
         JOIN workspaces w ON w.id = wm.workspace_id JOIN users u ON u.id = wm.member_id \
         WHERE wm.id = $1 AND w.slug = $2 AND u.is_bot = false \
         AND wm.is_active = true AND wm.deleted_at IS NULL",
    )
    .bind(pk)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some((target_row, target_user, target_role)) = target else {
        return Ok(missing());
    };
    let requester: Option<(uuid::Uuid, i16, uuid::Uuid)> = sqlx::query_as(
        "SELECT wm.id, wm.role, wm.workspace_id FROM workspace_members wm \
         JOIN workspaces w ON w.id = wm.workspace_id \
         WHERE w.slug = $1 AND wm.member_id = $2 AND wm.is_active = true AND wm.deleted_at IS NULL",
    )
    .bind(&slug)
    .bind(auth.0)
    .fetch_optional(&st.pool)
    .await?;
    let Some((requester_row, requester_role, workspace_id)) = requester else {
        return Ok(deny());
    };
    // `member.py:110-114` — self removal must use leave.
    if target_row == requester_row {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": SELF_REMOVE_MSG})),
        ));
    }
    // `member.py:116-120` — cannot remove a higher role.
    if let Err(e) = guard_destroy_role(requester_role, target_role) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
    }
    // `member.py:122-141` — sole-project-admin target (user-id version of
    // the annotate; see `sole_project_admin_exists`).
    if sole_project_admin_exists(&st.pool, workspace_id, target_user).await? {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": SOLE_PROJ_ADMIN_USER_MSG})),
        ));
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE project_members pm SET is_active = false, updated_at = now() \
         FROM workspaces w WHERE pm.workspace_id = w.id AND w.slug = $1 \
         AND pm.member_id = $2 AND pm.is_active = true",
    )
    .bind(&slug)
    .bind(target_user)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE workspace_members SET is_active = false, updated_at = now() WHERE id = $1")
        .bind(pk)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

/// Mirrors `WorkSpaceMemberViewSet.leave`
/// (`plane/app/views/workspace/member.py:160-205`): gate AMG; sole-ws-admin
/// → 400 (`:165-174`); sole-project-admin → 400 (`:176-195`); else
/// SOFT-deactivate project + ws rows → **204**. Multi-write in one tx.
pub async fn ws_leave(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let Some(role) = gate_ws_amg(&st.pool, auth.0, &slug).await? else {
        return Ok(deny());
    };
    let ws: Option<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL",
    )
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some((workspace_id,)) = ws else {
        return Ok(missing());
    };
    // `member.py:165-174` — sole ws admin cannot leave.
    if role == 20 {
        let admin_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM workspace_members \
             WHERE workspace_id = $1 AND role = 20 AND is_active = true AND deleted_at IS NULL",
        )
        .bind(workspace_id)
        .fetch_one(&st.pool)
        .await?;
        if admin_count.0 <= 1 {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": SOLE_WS_ADMIN_LEAVE_MSG})),
            ));
        }
    }
    // `member.py:176-195` — sole project admin cannot leave.
    if sole_project_admin_exists(&st.pool, workspace_id, auth.0).await? {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": SOLE_PROJ_ADMIN_LEAVE_MSG})),
        ));
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE project_members SET is_active = false, updated_at = now() \
         WHERE workspace_id = $1 AND member_id = $2 AND is_active = true",
    )
    .bind(workspace_id)
    .bind(auth.0)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE workspace_members SET is_active = false, updated_at = now() \
         WHERE workspace_id = $1 AND member_id = $2 AND is_active = true",
    )
    .bind(workspace_id)
    .bind(auth.0)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

// ============================================================================
// E5b — workspace member me + workspace project-members map.
// ============================================================================

/// Mirrors `WorkspaceMemberUserEndpoint.get`
/// (`plane/app/views/workspace/member.py:217-234`): NO membership gate
/// (plain `BaseAPIView`, `IsAuthenticated` only); 200 full row +
/// `draft_issue_count`; NON-member → 200 null (Django serializes
/// `.first()` → None as null — preserved, NOT a 404).
pub async fn ws_me(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let row: Option<WsMemberRow> = sqlx::query_as(&format!(
        "SELECT {WS_MEMBER_COLS} FROM workspace_members wm \
         JOIN workspaces w ON w.id = wm.workspace_id \
         JOIN users u ON u.id = wm.member_id \
         WHERE w.slug = $1 AND wm.member_id = $2 AND wm.is_active = true \
         AND wm.deleted_at IS NULL"
    ))
    .bind(&slug)
    .bind(auth.0)
    .fetch_optional(&st.pool)
    .await?;
    let Some(row) = row else {
        return Ok((StatusCode::OK, Json(Value::Null)));
    };
    // `member.py:221-226` — drafts created by the user in this workspace.
    let draft_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM draft_issues \
         WHERE created_by_id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
    )
    .bind(auth.0)
    .bind(row.workspace_id)
    .fetch_one(&st.pool)
    .await?;
    Ok((StatusCode::OK, Json(ws_me_json(&row, draft_count.0))))
}

/// Mirrors `WorkspaceProjectMemberEndpoint.get`
/// (`plane/app/views/workspace/member.py:237-266`): gate
/// `WorkspaceEntityPermission` (any ACTIVE ws member; non-member → DRF 403
/// `{"detail": ...}`, `permissions/workspace.py:74-82`); 200
/// `{project_id: [{id, role, member, project, original_role, created_at}]}`.
pub async fn ws_project_members(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        let e = deny_detail();
        return Ok(e);
    }
    // Projects where the requester is an active member (`:245-249`).
    let project_ids: Vec<uuid::Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT pm.project_id FROM project_members pm \
         JOIN workspaces w ON w.id = pm.workspace_id \
         WHERE pm.member_id = $1 AND w.slug = $2 AND pm.is_active = true AND pm.deleted_at IS NULL",
    )
    .bind(auth.0)
    .bind(&slug)
    .fetch_all(&st.pool)
    .await?;
    // All active members of those projects in this workspace (`:252-254`).
    let rows: Vec<PmShortRow> = if project_ids.is_empty() {
        vec![]
    } else {
        sqlx::query_as(
            "SELECT pm.id, pm.member_id, pm.role, pm.project_id, pm.created_at \
             FROM project_members pm JOIN workspaces w ON w.id = pm.workspace_id \
             WHERE w.slug = $1 AND pm.project_id = ANY($2) AND pm.is_active = true \
             AND pm.deleted_at IS NULL ORDER BY pm.created_at DESC",
        )
        .bind(&slug)
        .bind(&project_ids)
        .fetch_all(&st.pool)
        .await?
    };
    // `{project_id: [...]}` with `project` popped into the key
    // (`:257-264`).
    let mut map = serde_json::Map::new();
    for r in &rows {
        map.entry(r.project_id.to_string())
            .or_insert_with(|| Value::Array(vec![]))
            .as_array_mut()
            .expect("project members value is array")
            .push(pm_role_json(r));
    }
    // Strip the `project` key from each entry (Django `pop("project")`).
    for entries in map.values_mut() {
        for entry in entries.as_array_mut().expect("entries is array") {
            entry.as_object_mut().expect("entry is object").remove("project");
        }
    }
    Ok((StatusCode::OK, Json(Value::Object(map))))
}

// ============================================================================
// E5e — project members.
// ============================================================================

/// Default `view_props`/`default_props` (`get_default_props`,
/// `db/models/project.py:39-60`) and `preferences`
/// (`get_default_preferences`, `:64-65`) for bulk-created rows.
fn default_member_props() -> Value {
    json!({
        "filters": {"priority": null, "state": null, "state_group": null, "assignees": null,
            "created_by": null, "labels": null, "start_date": null, "target_date": null,
            "subscriber": null},
        "display_filters": {"group_by": null, "order_by": "-created_at", "type": null,
            "sub_issue": true, "show_empty_groups": true, "layout": "list",
            "calendar_date_range": ""},
    })
}

fn default_member_preferences() -> Value {
    json!({"pages": {"block_display": true},
        "navigation": {"default_tab": "work_items", "hide_in_more_menu": []}})
}

/// Default `ProjectUserProperty` columns (`db/models/issue.py:47-87` +
/// `get_default_preferences`, `sort_order=65535`).
fn default_user_props() -> (Value, Value, Value, Value) {
    let filters = json!({"priority": null, "state": null, "state_group": null, "assignees": null,
        "created_by": null, "labels": null, "start_date": null, "target_date": null,
        "subscriber": null});
    let display_filters = json!({"group_by": null, "order_by": "-created_at", "type": null,
        "sub_issue": true, "show_empty_groups": true, "layout": "list", "calendar_date_range": ""});
    let display_properties = json!({"assignee": true, "attachment_count": true, "created_on": true,
        "due_date": true, "estimate": true, "key": true, "labels": true, "link": true,
        "priority": true, "start_date": true, "state": true, "sub_issue_count": true,
        "updated_on": true});
    (filters, display_filters, display_properties, default_member_preferences())
}

/// Mirrors `ProjectMemberViewSet.create`
/// (`plane/app/views/project/member.py:46-154`): gate ADMIN project-level;
/// `{members:[{member_id,role?}]}` bulk with default role 5 (`:118`);
/// existing rows reactivated + role-updated (`:86-95`); new rows +
/// `ProjectUserProperty` rows bulk-created `ignore_conflicts` (`:134-136`);
/// celery email SKIPPED; **201** role-serializer array (`:152-154`).
///
/// Deviations (documented): Django `.get()` misses (unknown project /
/// non-ws `member_id`) would 404/500 — Rust answers `missing()` 404 for
/// both; a missing role on a REACTIVATED row defaults to 5 (Django writes
/// NULL → `IntegrityError` 400); non-coercible roles → 400 valid-detail
/// (Django `ValueError` → 500).
pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let Some(workspace_id) = project_in_workspace(&st.pool, project_id, &slug).await? else {
        return Ok(missing());
    };
    if gate_project(&st.pool, auth.0, &slug, project_id, true).await?.is_none() {
        return Ok(deny());
    }
    let entries = match parse_members_body(&body) {
        Ok(e) => e,
        Err(MembersParseError::Empty) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": MEMBERS_REQUIRED_MSG})),
            ));
        }
        Err(MembersParseError::Invalid) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": VALID_DETAIL_MSG})),
            ));
        }
        // Django `.get(member=None)` → `DoesNotExist` → 404.
        Err(MembersParseError::NullId) => return Ok(missing()),
    };
    // Workspace-role gates (`member.py:69-83`): ws ADMIN(20) invitees take
    // only 20; ws GUEST(5) invitees take only 5/None→5. A non-ws member
    // misses the `.get()` (`:70-72`) → 404 here (Django `DoesNotExist`).
    let mut roles: Vec<i16> = Vec::with_capacity(entries.len());
    for e in &entries {
        let ws_role: Option<i16> = sqlx::query_scalar(
            "SELECT wm.role FROM workspace_members wm \
             WHERE wm.workspace_id = $1 AND wm.member_id = $2 \
             AND wm.is_active = true AND wm.deleted_at IS NULL",
        )
        .bind(workspace_id)
        .bind(e.member_id)
        .fetch_optional(&st.pool)
        .await?;
        let Some(ws_role) = ws_role else {
            return Ok(missing());
        };
        let role = e.role.unwrap_or(5);
        // `member.py:73-77`.
        if ws_role == 20 && (role == 5 || role == 15) {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": WS_ROLE_LOWER_MSG})),
            ));
        }
        // `member.py:79-83`.
        if ws_role == 5 && (role == 15 || role == 20) {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": WS_ROLE_HIGHER_MSG})),
            ));
        }
        roles.push(role);
    }
    let member_ids: Vec<uuid::Uuid> = entries.iter().map(|e| e.member_id).collect();
    let mut tx = st.pool.begin().await?;
    // `member.py:86-95` — reactivate existing rows with the new roles.
    for (e, role) in entries.iter().zip(roles.iter()) {
        sqlx::query(
            "UPDATE project_members SET role = $1, is_active = true, updated_at = now() \
             WHERE project_id = $2 AND member_id = $3 AND deleted_at IS NULL",
        )
        .bind(role)
        .bind(project_id)
        .bind(e.member_id)
        .execute(&mut *tx)
        .await?;
    }
    // `member.py:98-107` — min sort_order per member for property rows.
    let sort_rows: Vec<(uuid::Uuid, Option<f64>)> = sqlx::query_as(
        "SELECT user_id, MIN(sort_order) FROM project_user_properties \
         WHERE workspace_id = $1 AND user_id = ANY($2) AND deleted_at IS NULL GROUP BY user_id",
    )
    .bind(workspace_id)
    .bind(&member_ids)
    .fetch_all(&mut *tx)
    .await?;
    let sort_map: std::collections::HashMap<uuid::Uuid, Option<f64>> =
        sort_rows.into_iter().collect();
    let member_props = default_member_props();
    let (filters, display_filters, display_properties, user_prefs) = default_user_props();
    for (e, role) in entries.iter().zip(roles.iter()) {
        // `member.py:115-122` — new member rows (`ignore_conflicts`, `:134`).
        sqlx::query(
            "INSERT INTO project_members (id, member_id, role, project_id, workspace_id, \
             created_by_id, view_props, default_props, preferences, sort_order, is_active, \
             created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $6, $7, 65535, true, now(), now()) \
             ON CONFLICT DO NOTHING",
        )
        .bind(e.member_id)
        .bind(role)
        .bind(project_id)
        .bind(workspace_id)
        .bind(auth.0)
        .bind(&member_props)
        .bind(default_member_preferences())
        .execute(&mut *tx)
        .await?;
        // `member.py:124-131` — property rows (`ignore_conflicts`, `:136`).
        let sort_order = match sort_map.get(&e.member_id).copied().flatten() {
            Some(m) => m - 10000.0,
            None => 65535.0,
        };
        sqlx::query(
            "INSERT INTO project_user_properties (id, project_id, user_id, workspace_id, \
             created_by_id, filters, display_filters, display_properties, rich_filters, \
             preferences, sort_order, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, '{}', $8, $9, now(), now()) \
             ON CONFLICT DO NOTHING",
        )
        .bind(project_id)
        .bind(e.member_id)
        .bind(workspace_id)
        .bind(auth.0)
        .bind(&filters)
        .bind(&display_filters)
        .bind(&display_properties)
        .bind(&user_prefs)
        .bind(sort_order)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    // `member.py:138-154` — re-read + `ProjectMemberRoleSerializer[]`, 201.
    let rows: Vec<PmShortRow> = sqlx::query_as(
        "SELECT pm.id, pm.member_id, pm.role, pm.project_id, pm.created_at \
         FROM project_members pm WHERE pm.project_id = $1 AND pm.member_id = ANY($2) \
         AND pm.deleted_at IS NULL ORDER BY pm.created_at DESC",
    )
    .bind(project_id)
    .bind(&member_ids)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(json!(rows.iter().map(pm_role_json).collect::<Vec<_>>())),
    ))
}

/// Mirrors `ProjectMemberViewSet.list`
/// (`plane/app/views/project/member.py:156-169`): gate AMG project-level;
/// 200 `[{id, member, role}]` over active non-bot members whose user holds
/// an active ws membership (`:159-166`).
pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if project_in_workspace(&st.pool, project_id, &slug).await?.is_none() {
        return Ok(missing());
    }
    if gate_project(&st.pool, auth.0, &slug, project_id, false).await?.is_none() {
        return Ok(deny());
    }
    let rows: Vec<PmShortRow> = sqlx::query_as(
        "SELECT pm.id, pm.member_id, pm.role, pm.project_id, pm.created_at \
         FROM project_members pm JOIN workspaces w ON w.id = pm.workspace_id \
         JOIN users u ON u.id = pm.member_id \
         WHERE pm.project_id = $1 AND w.slug = $2 AND u.is_bot = false \
         AND pm.is_active = true AND pm.deleted_at IS NULL \
         AND EXISTS(SELECT 1 FROM workspace_members wm WHERE wm.workspace_id = w.id \
           AND wm.member_id = pm.member_id AND wm.is_active = true AND wm.deleted_at IS NULL) \
         ORDER BY pm.created_at DESC",
    )
    .bind(project_id)
    .bind(&slug)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!(rows.iter().map(pm_role_short_json).collect::<Vec<_>>())),
    ))
}

/// Compact member-id list (pre-E5 `project-members-lite/` shape — kept
/// byte-identical; E5 does not cover it).
pub async fn list_lite(
    State(st): State<AppState>,
    _auth: AuthUser,
    Path((_slug, _project_id)): Path<(String, uuid::Uuid)>,
) -> Result<Json<Vec<Value>>, common::errors::AppError> {
    let ids: Vec<uuid::Uuid> = sqlx::query_scalar(
        "SELECT member_id FROM project_members WHERE project_id = $1 AND member_id IS NOT NULL AND deleted_at IS NULL",
    )
    .bind(_project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(ids.into_iter().map(|id| json!({"id": id})).collect()))
}

async fn fetch_pm_full(
    pool: &sqlx::PgPool,
    pk: uuid::Uuid,
    project_id: uuid::Uuid,
    slug: &str,
) -> Result<Option<PmFullRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {PM_FULL_COLS} FROM project_members pm \
         JOIN workspaces w ON w.id = pm.workspace_id \
         JOIN projects p ON p.id = pm.project_id \
         LEFT JOIN users u ON u.id = pm.member_id \
         WHERE pm.id = $1 AND pm.project_id = $2 AND w.slug = $3 \
         AND pm.is_active = true AND pm.deleted_at IS NULL"
    ))
    .bind(pk)
    .bind(project_id)
    .bind(slug)
    .fetch_optional(pool)
    .await
}

/// Mirrors `ProjectMemberViewSet.retrieve`
/// (`plane/app/views/project/member.py:171-203`): gate AMG; requester miss
/// → 404 `missing()` (Django `.get()`, `:173-178`); target miss → 404
/// `{"error":"Project member not found"}` (`:192-196`); requester role > 5
/// → admin shape, else `(id, member, role)` (`:198-201`).
pub async fn detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if project_in_workspace(&st.pool, project_id, &slug).await?.is_none() {
        return Ok(missing());
    }
    let Some(requester_role) = gate_project(&st.pool, auth.0, &slug, project_id, false).await?
    else {
        return Ok(deny());
    };
    // Django re-gets the requester row (`:173-178`) — a gate pass implies it.
    let row = fetch_pm_full(&st.pool, pk, project_id, &slug).await?;
    let Some(row) = row else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": PROJ_MEMBER_NOT_FOUND_MSG})),
        ));
    };
    // Bot members are excluded from retrieve (`:185`); the LEFT JOIN keeps
    // the row — enforce here. (Django `.first()` → None → 404.)
    if row.u_is_bot == Some(true) {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": PROJ_MEMBER_NOT_FOUND_MSG})),
        ));
    }
    if sees_admin_shape(requester_role) {
        Ok((StatusCode::OK, Json(pm_full_json(&row, true))))
    } else {
        Ok((
            StatusCode::OK,
            Json(json!({"id": row.id, "member": opt_uuid(&row.member_id), "role": row.role})),
        ))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PatchMember {
    pub role: Option<Value>,
    #[serde(default)]
    pub is_active: Option<Value>,
    #[serde(default)]
    pub comment: Option<Value>,
}

/// Mirrors `ProjectMemberViewSet.partial_update` with the FULL gate ladder
/// (`plane/app/views/project/member.py:205-288`): gate AMG; target / target
/// ws-role / requester ws-role / requester project-row misses → 404
/// `missing()` (Django `.get()`s, `:207-231`); ladder → 400/403 verbatim
/// (see [`project_patch_decision`]); `ProjectMemberSerializer` errors →
/// 400; 200 full lite-member shape (`:285-287`).
pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<PatchMember>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if project_in_workspace(&st.pool, project_id, &slug).await?.is_none() {
        return Ok(missing());
    }
    if gate_project(&st.pool, auth.0, &slug, project_id, false).await?.is_none() {
        return Ok(deny());
    }
    // `member.py:207` — target (no bot filter on this path — preserved).
    let target: Option<(Option<uuid::Uuid>, i16)> = sqlx::query_as(
        "SELECT pm.member_id, pm.role FROM project_members pm \
         JOIN workspaces w ON w.id = pm.workspace_id \
         WHERE pm.id = $1 AND w.slug = $2 AND pm.project_id = $3 \
         AND pm.is_active = true AND pm.deleted_at IS NULL",
    )
    .bind(pk)
    .bind(&slug)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some((target_user, target_role)) = target else {
        return Ok(missing());
    };
    // `member.py:210-216` — target + requester ws roles.
    let target_ws_role: Option<i16> = match target_user {
        Some(uid) => ws_role(&st.pool, uid, &slug).await?,
        None => None,
    };
    let Some(target_ws_role) = target_ws_role else {
        return Ok(missing());
    };
    let requester_ws_role = ws_role(&st.pool, auth.0, &slug).await?;
    let Some(requester_ws_role) = requester_ws_role else {
        return Ok(missing());
    };
    let is_ws_admin = requester_ws_role == 20;
    // `member.py:226-231` — requester project row.
    let requester: Option<(i16,)> = sqlx::query_as(
        "SELECT pm.role FROM project_members pm JOIN workspaces w ON w.id = pm.workspace_id \
         WHERE pm.project_id = $1 AND w.slug = $2 AND pm.member_id = $3 \
         AND pm.is_active = true AND pm.deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(&slug)
    .bind(auth.0)
    .fetch_optional(&st.pool)
    .await?;
    let Some((requester_role,)) = requester else {
        return Ok(missing());
    };
    // `"role" in request.data` (`:233`) — explicit null counts as present
    // (Django) but fails `int(None)` → 500; sane 400 valid-detail here.
    let new_role: Option<i16> = match &body.role {
        None => None,
        Some(Value::Null) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": VALID_DETAIL_MSG})),
            ));
        }
        Some(v) => match coerce_role_value(v) {
            Ok(r) => Some(r),
            Err(()) => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": VALID_DETAIL_MSG})),
                ));
            }
        },
    };
    if let Some(r) = new_role {
        if !ROLES.contains(&r) {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"role": [invalid_choice_msg(&r.to_string())]})),
            ));
        }
    }
    let touches_active = body.is_active.is_some();
    let ctx = PatchGateCtx {
        is_self: target_user == Some(auth.0),
        is_ws_admin,
        requester_role,
        target_role,
        new_role,
        target_ws_role,
        touches_active,
    };
    match project_patch_decision(&ctx) {
        Ok(()) => {}
        Err(PatchDeny::BadRequest(m)) => {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": m}))));
        }
        Err(PatchDeny::Forbidden(m)) => {
            return Ok((StatusCode::FORBIDDEN, Json(json!({"error": m}))));
        }
    }
    // `is_active` parses like DRF `BooleanField`; garbage → 400 field errors.
    let new_active: Option<bool> = match &body.is_active {
        None => None,
        Some(v) => match parse_drf_bool(v) {
            Some(b) => Some(b),
            None => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"is_active": ["Must be a valid boolean."]})),
                ));
            }
        },
    };
    // `comment` is free text (`TextField(blank=True, null=True)`).
    let new_comment: Option<Option<String>> = match &body.comment {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(s)) => Some(Some(s.clone())),
        Some(_) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"comment": ["Not a valid string."]})),
            ));
        }
    };
    sqlx::query(
        "UPDATE project_members pm SET role = COALESCE($1, pm.role), \
         is_active = COALESCE($2, pm.is_active), comment = COALESCE($3, pm.comment), \
         updated_at = now() FROM workspaces w \
         WHERE pm.id = $4 AND pm.workspace_id = w.id AND w.slug = $5",
    )
    .bind(new_role)
    .bind(new_active)
    .bind(new_comment.clone().unwrap_or(None))
    .bind(pk)
    .bind(&slug)
    .execute(&st.pool)
    .await?;
    // Explicit-null comment clear (COALESCE cannot express it).
    if new_comment == Some(None) {
        sqlx::query("UPDATE project_members SET comment = NULL, updated_at = now() WHERE id = $1")
            .bind(pk)
            .execute(&st.pool)
            .await?;
    }
    match fetch_pm_full(&st.pool, pk, project_id, &slug).await? {
        Some(row) => Ok((StatusCode::OK, Json(pm_full_json(&row, false)))),
        None => Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": PROJ_MEMBER_NOT_FOUND_MSG})),
        )),
    }
}

/// DRF `BooleanField` truth table (true/false + 1/0 + common spellings).
pub fn parse_drf_bool(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => match n.as_i64() {
            Some(1) => Some(true),
            Some(0) => Some(false),
            _ => None,
        },
        Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "y" | "t" | "on" => Some(true),
            "false" | "0" | "no" | "n" | "f" | "off" => Some(false),
            _ => None,
        },
        Value::Null => None,
        _ => None,
    }
}

/// Mirrors `ProjectMemberViewSet.destroy`
/// (`plane/app/views/project/member.py:290-321`): gate ADMIN; self → 400
/// sic-workspace message (`:307-311`); lower → 400 (`:313-317`);
/// SOFT-deactivate (`is_active = false`, `:319-320` — never hard delete) →
/// **204**.
pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if project_in_workspace(&st.pool, project_id, &slug).await?.is_none() {
        return Ok(missing());
    }
    if gate_project(&st.pool, auth.0, &slug, project_id, true).await?.is_none() {
        return Ok(deny());
    }
    // Target with bot exclusion (`:292-298`).
    let target: Option<(uuid::Uuid, Option<uuid::Uuid>, i16)> = sqlx::query_as(
        "SELECT pm.id, pm.member_id, pm.role FROM project_members pm \
         JOIN workspaces w ON w.id = pm.workspace_id \
         LEFT JOIN users u ON u.id = pm.member_id \
         WHERE pm.id = $1 AND w.slug = $2 AND pm.project_id = $3 \
         AND (u.id IS NULL OR u.is_bot = false) \
         AND pm.is_active = true AND pm.deleted_at IS NULL",
    )
    .bind(pk)
    .bind(&slug)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some((target_row, _, target_role)) = target else {
        return Ok(missing());
    };
    let requester: Option<(uuid::Uuid, i16)> = sqlx::query_as(
        "SELECT pm.id, pm.role FROM project_members pm \
         JOIN workspaces w ON w.id = pm.workspace_id \
         WHERE w.slug = $1 AND pm.member_id = $2 AND pm.project_id = $3 \
         AND pm.is_active = true AND pm.deleted_at IS NULL",
    )
    .bind(&slug)
    .bind(auth.0)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some((requester_row, requester_role)) = requester else {
        return Ok(missing());
    };
    // `member.py:307-311` — sic "workspace" message on the project path.
    if target_row == requester_row {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": SELF_REMOVE_MSG})),
        ));
    }
    // `member.py:313-317`.
    if let Err(e) = guard_destroy_role(requester_role, target_role) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
    }
    sqlx::query("UPDATE project_members SET is_active = false, updated_at = now() WHERE id = $1")
        .bind(pk)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

/// Mirrors `ProjectMemberViewSet.leave`
/// (`plane/app/views/project/member.py:323-349`): gate AMG project-level
/// (any ACTIVE membership — a missing/inactive row 403s via `deny()`; the
/// inner `.get(is_active=True)` 404 is unreachable except on races,
/// preserved as `missing()`); sole active admin → 400 sic message
/// (`guard_leave`, `:332-345`); else `is_active = false` → **204**.
pub async fn leave_project(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if project_in_workspace(&st.pool, project_id, &slug).await?.is_none() {
        return Ok(missing());
    }
    let Some(role) = gate_project(&st.pool, auth.0, &slug, project_id, false).await? else {
        return Ok(deny());
    };
    let admin_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM project_members WHERE project_id = $1 AND role = 20 AND is_active = true AND deleted_at IS NULL",
    )
    .bind(project_id)
    .fetch_one(&st.pool)
    .await?;
    if let Err(e) = guard_leave(role == 20, admin_count) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
    }
    let n = sqlx::query(
        "UPDATE project_members SET is_active = false, updated_at = now() WHERE project_id = $1 AND member_id = $2 AND is_active = true",
    )
    .bind(project_id)
    .bind(auth.0)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        // Race: membership deactivated between gate and update. Django's
        // `get()` would 404 here (`ObjectDoesNotExist`).
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

// ============================================================================
// E5e — project member preferences.
// ============================================================================

#[derive(Debug, Clone, sqlx::FromRow)]
struct PrefRow {
    preferences: Value,
    project_id: uuid::Uuid,
    member_id: Option<uuid::Uuid>,
    workspace_id: uuid::Uuid,
}

async fn fetch_pref(
    pool: &sqlx::PgPool,
    slug: &str,
    project_id: uuid::Uuid,
    member_id: uuid::Uuid,
) -> Result<Option<PrefRow>, sqlx::Error> {
    // `get_queryset` (`member.py:383-388`): no `is_active` filter — preserved.
    sqlx::query_as(
        "SELECT pm.preferences, pm.project_id, pm.member_id, pm.workspace_id \
         FROM project_members pm JOIN workspaces w ON w.id = pm.workspace_id \
         WHERE pm.project_id = $1 AND pm.member_id = $2 AND w.slug = $3 AND pm.deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(member_id)
    .bind(slug)
    .fetch_optional(pool)
    .await
}

fn pref_json(r: &PrefRow) -> Value {
    json!({
        "preferences": r.preferences,
        "project_id": r.project_id,
        "member_id": opt_uuid(&r.member_id),
        "workspace_id": r.workspace_id,
    })
}

/// Mirrors `ProjectMemberPreferenceEndpoint.get`
/// (`plane/app/views/project/member.py:402-408`): gate AMG; 200
/// `{preferences, project_id, member_id, workspace_id}`; miss → 404.
pub async fn pref_get(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, member_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if project_in_workspace(&st.pool, project_id, &slug).await?.is_none() {
        return Ok(missing());
    }
    if gate_project(&st.pool, auth.0, &slug, project_id, false).await?.is_none() {
        return Ok(deny());
    }
    match fetch_pref(&st.pool, &slug, project_id, member_id).await? {
        Some(r) => Ok((StatusCode::OK, Json(pref_json(&r)))),
        None => Ok(missing()),
    }
}

/// Mirrors `ProjectMemberPreferenceEndpoint.patch`
/// (`plane/app/views/project/member.py:390-400`): gate AMG; the body IS the
/// preferences object (merged shallow, `validate_preferences`,
/// `serializers/project.py:166-175`); 200 `{"preferences": merged}`;
/// miss → 404.
pub async fn pref_patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, member_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if project_in_workspace(&st.pool, project_id, &slug).await?.is_none() {
        return Ok(missing());
    }
    if gate_project(&st.pool, auth.0, &slug, project_id, false).await?.is_none() {
        return Ok(deny());
    }
    let Some(cur) = fetch_pref(&st.pool, &slug, project_id, member_id).await? else {
        return Ok(missing());
    };
    let merged = match merge_preferences(&cur.preferences, &body) {
        Ok(m) => m,
        Err(()) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": VALID_DETAIL_MSG})),
            ));
        }
    };
    sqlx::query(
        "UPDATE project_members pm SET preferences = $1, updated_at = now() \
         FROM workspaces w WHERE pm.project_id = $2 AND pm.member_id = $3 \
         AND pm.workspace_id = w.id AND w.slug = $4 AND pm.deleted_at IS NULL",
    )
    .bind(&merged)
    .bind(project_id)
    .bind(member_id)
    .bind(&slug)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::OK, Json(json!({"preferences": merged}))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_shape_iff_requester_above_guest() {
        // `workspace/member.py:51,70` + `project/member.py:198`.
        assert!(sees_admin_shape(20));
        assert!(sees_admin_shape(15));
        assert!(!sees_admin_shape(5));
    }

    #[test]
    fn patch_ladder_self_update() {
        // `project/member.py:220-224` — verbatim 400.
        let ctx = PatchGateCtx {
            is_self: true,
            is_ws_admin: false,
            requester_role: 15,
            target_role: 15,
            new_role: Some(5),
            target_ws_role: 15,
            touches_active: false,
        };
        assert_eq!(
            project_patch_decision(&ctx).unwrap_err(),
            PatchDeny::BadRequest("You cannot update your own role")
        );
        // Ws-admin bypasses the self rule (`not is_workspace_admin`).
        let ctx = PatchGateCtx { is_ws_admin: true, ..ctx };
        assert!(project_patch_decision(&ctx).is_ok());
    }

    #[test]
    fn patch_ladder_role_block_matrix() {
        // `project/member.py:235-239` — verbatim 403.
        let base = PatchGateCtx {
            is_self: false,
            is_ws_admin: false,
            requester_role: 15,
            target_role: 5,
            new_role: Some(5),
            target_ws_role: 15,
            touches_active: false,
        };
        assert_eq!(
            project_patch_decision(&base).unwrap_err(),
            PatchDeny::Forbidden("You do not have permission to update roles")
        );
        // `project/member.py:242-246` — target ≥ requester, verbatim 403.
        let ctx = PatchGateCtx { requester_role: 20, ..base };
        let ctx = PatchGateCtx { target_role: 20, ..ctx };
        assert_eq!(
            project_patch_decision(&ctx).unwrap_err(),
            PatchDeny::Forbidden(
                "You cannot update the role of a member with a role equal to or higher than your own"
            )
        );
        // `project/member.py:251-255` — new ≥ requester, verbatim 403.
        let ctx = PatchGateCtx { target_role: 5, new_role: Some(20), ..ctx };
        assert_eq!(
            project_patch_decision(&ctx).unwrap_err(),
            PatchDeny::Forbidden("You cannot assign a role equal to or higher than your own")
        );
        // `project/member.py:258-262` — ws-guest cap, verbatim 400 (no
        // ws-admin bypass on this branch).
        let ctx = PatchGateCtx { new_role: Some(15), target_ws_role: 5, ..ctx };
        assert_eq!(
            project_patch_decision(&ctx).unwrap_err(),
            PatchDeny::BadRequest("You cannot add a user with role higher than the workspace role")
        );
        let ctx = PatchGateCtx { is_ws_admin: true, ..ctx };
        assert_eq!(
            project_patch_decision(&ctx).unwrap_err(),
            PatchDeny::BadRequest("You cannot add a user with role higher than the workspace role")
        );
        // Clean admin update passes.
        let ctx = PatchGateCtx {
            is_self: false,
            is_ws_admin: false,
            requester_role: 20,
            target_role: 5,
            new_role: Some(15),
            target_ws_role: 15,
            touches_active: false,
        };
        assert!(project_patch_decision(&ctx).is_ok());
    }

    #[test]
    fn patch_ladder_active_block_matrix() {
        // `project/member.py:271-275` — verbatim 403.
        let ctx = PatchGateCtx {
            is_self: false,
            is_ws_admin: false,
            requester_role: 15,
            target_role: 5,
            new_role: None,
            target_ws_role: 15,
            touches_active: true,
        };
        assert_eq!(
            project_patch_decision(&ctx).unwrap_err(),
            PatchDeny::Forbidden("You do not have permission to update member status")
        );
        // `project/member.py:277-281` — verbatim 403.
        let ctx = PatchGateCtx { requester_role: 20, target_role: 20, ..ctx };
        assert_eq!(
            project_patch_decision(&ctx).unwrap_err(),
            PatchDeny::Forbidden(
                "You cannot update the status of a member with a role equal to or higher than your own"
            )
        );
        // No `is_active` key → block skipped even for guests.
        let ctx = PatchGateCtx { requester_role: 5, target_role: 5, touches_active: false, ..ctx };
        assert!(project_patch_decision(&ctx).is_ok());
    }

    #[test]
    fn members_body_parsing() {
        // Empty → `member.py:55-59` verbatim bucket.
        assert_eq!(
            parse_members_body(&json!({})).unwrap_err(),
            MembersParseError::Empty
        );
        assert_eq!(
            parse_members_body(&json!({"members": []})).unwrap_err(),
            MembersParseError::Empty
        );
        // Null id → Django `.get(member=None)` → 404 bucket.
        let id = uuid::Uuid::new_v4();
        assert_eq!(
            parse_members_body(&json!({"members": [{"member_id": null}]})).unwrap_err(),
            MembersParseError::NullId
        );
        // Garbage id → 400 bucket.
        assert_eq!(
            parse_members_body(&json!({"members": [{"member_id": "nope"}]})).unwrap_err(),
            MembersParseError::Invalid
        );
        // Role defaults to None (= 5 at insert, `member.py:118`).
        let entries = parse_members_body(&json!({"members": [{"member_id": id}]})).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].role, None);
        // Integer strings coerce (Django `int(...)`).
        let entries =
            parse_members_body(&json!({"members": [{"member_id": id, "role": "15"}]})).unwrap();
        assert_eq!(entries[0].role, Some(15));
    }

    #[test]
    fn preferences_merge_is_shallow_update() {
        // `serializers/project.py:166-175` — `dict.update` semantics.
        let stored = json!({"a": 1, "b": {"x": 1}});
        let merged = merge_preferences(&stored, &json!({"b": {"y": 2}, "c": 3})).unwrap();
        assert_eq!(merged, json!({"a": 1, "b": {"y": 2}, "c": 3}));
        assert!(merge_preferences(&stored, &json!([1])).is_err());
    }

    #[test]
    fn drf_bool_table() {
        assert_eq!(parse_drf_bool(&json!(true)), Some(true));
        assert_eq!(parse_drf_bool(&json!(0)), Some(false));
        assert_eq!(parse_drf_bool(&json!("off")), Some(false));
        assert_eq!(parse_drf_bool(&json!("maybe")), None);
    }
}

#[cfg(test)]
mod leave_tests {
    // Reuses T0 `guard_leave` (`crate::routes::project`) exactly — no second
    // helper. Role maps to `is_admin` (`role == 20`); the count is the
    // project-wide active-admin count.
    use crate::routes::project::guard_leave;

    #[test]
    fn sole_admin_leave_matrix_matches_django() {
        // Mirrors `plane/app/views/project/member.py:332-345`: verbatim 400
        // (grammar quirks included) when the leaver is the only active admin.
        assert_eq!(
            guard_leave(true, 1).unwrap_err(),
            "You cannot leave the project as your the only admin of the project you will have to either delete the project or create an another admin"
        );
        assert!(guard_leave(true, 2).is_ok());
        // Non-admins (15/5) never hit the guard — count is irrelevant.
        assert!(guard_leave(false, 99).is_ok());
        assert!(guard_leave(false, 1).is_ok());
    }

    #[test]
    fn leave_handler_exists_for_post_route() {
        // Wiring guard: `main.rs` registers
        // `POST .../members/leave/` → `leave_project` (Django
        // `urls/project.py:88-89` maps `{"post": "leave"}` only).
        let _ = super::leave_project;
    }
}
