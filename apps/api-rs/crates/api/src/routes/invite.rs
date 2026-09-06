//! Workspace + project invitations — parity with Django
//! `plane/app/views/workspace/invite.py:44-282` and
//! `plane/app/views/project/invite.py:40-286`.
//!
//! Token scheme (`workspace/invite.py:95-99`, `project/invite.py:83-87`):
//! `jwt.encode({"email": ..., "timestamp": ...}, SECRET_KEY, HS256)` over a
//! lower+stripped email with NO expiry. Rust mints with the existing JWT
//! infra (`JWT_SECRET`, `jsonwebtoken` HS256, no `exp`); join endpoints
//! verify by exact equality against the stored token (same as Django
//! `invite.token != token`, `workspace/invite.py:155`,
//! `project/invite.py:201`) — never 401 for token problems, only for anon.
//!
//! Celery/email sends are SKIPPED (rows + tokens created, success messages
//! kept). Invite revoke/accept-delete are HARD `DELETE`s (Django `.delete()`
//! with no soft override — `workspace/invite.py:130-133,209,281`; project
//! detail destroy via default DRF `destroy` → hard delete, same reason).
//! Member rows touched by joins deactivate/reactivate via `is_active`
//! (never `UPDATE deleted_at`).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::routes::member::{
    coerce_role_value, deny_detail, gate_project, is_valid_email, parse_drf_bool,
    project_in_workspace, ROLES, VALID_DETAIL_MSG,
};
use crate::routes::project::{deny, missing, ws_role};
use crate::routes::users_me::workspace_invite_link;
use crate::{middleware::auth::AuthUser, state::AppState};

// ============================================================================
// Error strings — every literal quoted from Django with file:line.
// ============================================================================

/// `plane/app/views/workspace/invite.py:56` and
/// `plane/app/views/project/invite.py:62` (same text both paths).
pub const EMAILS_REQUIRED_MSG: &str = "Emails are required";
/// `plane/app/views/workspace/invite.py:64` (invite role above requester).
pub const HIGHER_ROLE_MSG: &str = "You cannot invite a user with higher role";
/// `plane/app/views/workspace/invite.py:81` (already a ws member).
pub const ALREADY_MEMBER_MSG: &str = "Some users are already member of workspace";
/// `plane/app/views/workspace/invite.py:128` (ws bulk-create ok — 200, sic
/// plural; project twin `:116` is the singular below).
pub const WS_SENT_MSG: &str = "Emails sent successfully";
/// `plane/app/views/project/invite.py:116` (sic singular).
pub const PROJ_SENT_MSG: &str = "Email sent successfully";
/// `plane/app/views/workspace/invite.py:157` (join token mismatch).
pub const WS_JOIN_FORBIDDEN_MSG: &str = "You do not have permission to join the workspace";
/// `plane/app/views/workspace/invite.py:172` and
/// `plane/app/views/project/invite.py:218` (authed user ≠ invitee — same
/// text both paths).
pub const INVITE_EMAIL_MISMATCH_MSG: &str = "You do not have permission to accept this invitation";
/// `plane/app/views/workspace/invite.py:223` and
/// `plane/app/views/project/invite.py:278` (double respond — same text).
pub const ALREADY_RESPONDED_MSG: &str = "You have already responded to the invitation request";
/// `plane/app/views/workspace/invite.py:212` (ws accept).
pub const WS_ACCEPTED_MSG: &str = "Workspace Invitation Accepted";
/// `plane/app/views/workspace/invite.py:218` (ws reject).
pub const WS_REJECTED_MSG: &str = "Workspace Invitation was not accepted";
/// `plane/app/views/project/invite.py:70` (role vs ws role — INTENDED 400;
/// Django omits `status=` so it returns 200, documented below).
pub const DIFF_ROLE_MSG: &str = "You cannot invite a user with different role than workspace role";
/// `plane/app/views/project/invite.py:204` (join token mismatch).
pub const PROJ_JOIN_FORBIDDEN_MSG: &str = "You do not have permission to join the project";
/// `plane/app/views/project/invite.py:226` (non-bool `accepted`).
pub const ACCEPTED_BOOL_MSG: &str = "`accepted` must be a boolean";
/// `plane/app/views/project/invite.py:269` (project accept).
pub const PROJ_ACCEPTED_MSG: &str = "Project Invitation Accepted";
/// `plane/app/views/project/invite.py:275` (project reject).
pub const PROJ_REJECTED_MSG: &str = "Project Invitation was not accepted";

// ============================================================================
// Pure helpers (unit-tested below).
// ============================================================================

/// Email normalization applied at build time
/// (`workspace/invite.py:93`, `project/invite.py:80`): strip + lowercase.
pub fn normalize_email(raw: &str) -> String {
    raw.trim().to_lowercase()
}

/// Invalid-email body (`workspace/invite.py:105-110`,
/// `project/invite.py:93-98`):
/// `Invalid email - <obj> provided a valid email address is required to
/// send the invite` where `<obj>` is the Python `repr` of the entry dict.
/// Rust renders the closest equivalent: single-quoted pairs in the entry's
/// (alphabetical — `serde_json` maps sort keys) order.
pub fn invalid_email_msg(entry: &Value) -> String {
    format!(
        "Invalid email - {} provided a valid email address is required to send the invite",
        python_dict_repr(entry)
    )
}

fn python_dict_repr(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let parts: Vec<String> = m
                .iter()
                .map(|(k, v)| format!("'{k}': {}", python_scalar_repr(v)))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
        _ => python_scalar_repr(v),
    }
}

fn python_scalar_repr(v: &Value) -> String {
    match v {
        Value::String(s) => format!("'{s}'"),
        Value::Null => "None".to_string(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        other => other.to_string(),
    }
}

/// Role-higher gate (`workspace/invite.py:62`): any entry role (default 5)
/// above the requester's → 400.
pub fn guard_invite_role(requester_role: i16, entry_role: i16) -> Result<(), String> {
    if entry_role > requester_role {
        return Err(HIGHER_ROLE_MSG.to_string());
    }
    Ok(())
}

/// JWT claims for minted invite tokens: `{"email", "timestamp"}`, NO `exp`
/// (`workspace/invite.py:95-99`).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct InviteClaims {
    email: String,
    timestamp: f64,
}

/// Mints an invite token with the existing JWT infra (`JWT_SECRET`, HS256,
/// no expiry) — the Rust side of `jwt.encode(..., SECRET_KEY, "HS256")`.
/// (Deployment assumption, documented: Django signs with `SECRET_KEY`;
/// equality-against-stored-token is the only check on the join path, so a
/// `SECRET_KEY`/`JWT_SECRET` skew breaks nothing at accept time.)
pub fn mint_invite_token(email: &str, secret: &str) -> String {
    let claims = InviteClaims {
        email: email.to_string(),
        timestamp: chrono::Utc::now().timestamp() as f64,
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("invite jwt encode")
}

/// Verifies a minted token back to its email (round-trip unit surface;
/// `validate_exp` off + no required claims — Django mints no `exp`).
/// `#[allow(dead_code)]`: the join path verifies by stored-token equality
/// (Django `invite.token != token`); this is the construction point for
/// tests (E2 `cycle.rs` precedent).
#[allow(dead_code)]
pub fn verify_invite_token(token: &str, secret: &str) -> Result<String, String> {
    let mut validation = jsonwebtoken::Validation::default();
    validation.validate_exp = false;
    validation.required_spec_claims.clear();
    jsonwebtoken::decode::<InviteClaims>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|d| d.claims.email)
    .map_err(|e| e.to_string())
}

/// Workspace-join `accepted` truthiness (`workspace/invite.py:178`):
/// `request.data.get("accepted", False)` with NO bool check — Python
/// truthiness applies (`"false"` is truthy!). Missing/null/false → reject.
pub fn ws_accepted_truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Bool(true)) => true,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// WS-role cap for project-join workspace creation
/// (`project/invite.py:245`): `15 if invite.role >= 15 else invite.role`.
pub fn cap_role_for_ws(invite_role: i16) -> i16 {
    if invite_role >= 15 { 15 } else { invite_role }
}

/// One parsed invite entry: normalized email + role (default 5).
#[derive(Debug, Clone)]
pub struct InviteEntry {
    pub email: String,
    pub role: i16,
}

// ============================================================================
// Shared gates + lookups.
// ============================================================================

/// `WorkSpaceAdminPermission` (`plane/app/permissions/workspace.py:58-67` —
/// despite the name, ADMIN **and MEMBER** 20/15, active): invite CRUD gate.
/// Deny is the DRF permission-class 403 `{"detail": ...}`.
async fn gate_ws_invite(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
) -> Result<Option<i16>, sqlx::Error> {
    let role = ws_role(pool, user, slug).await?;
    Ok(match role {
        Some(20) | Some(15) => role,
        _ => None,
    })
}

async fn ws_id_by_slug(
    pool: &sqlx::PgPool,
    slug: &str,
) -> Result<Option<uuid::Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL")
        .bind(slug)
        .fetch_optional(pool)
        .await
}

// ============================================================================
// Row structs + JSON builders.
// ============================================================================

#[derive(Debug, Clone, sqlx::FromRow)]
struct WsInviteRow {
    id: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    email: String,
    accepted: bool,
    token: String,
    message: Option<String>,
    responded_at: Option<chrono::DateTime<chrono::Utc>>,
    role: i16,
    workspace_id: uuid::Uuid,
    ws_name: String,
    ws_slug: String,
    ws_logo: Option<String>,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
}

const WS_INVITE_COLS: &str = "i.id, i.created_at, i.updated_at, i.email, i.accepted, i.token, \
    i.message, i.responded_at, i.role, i.workspace_id, \
    w.name AS ws_name, w.slug AS ws_slug, w.logo AS ws_logo, \
    i.created_by_id, i.updated_by_id";

fn opt_uuid(u: &Option<uuid::Uuid>) -> Value {
    u.map(|v| json!(v)).unwrap_or(Value::Null)
}

fn ws_lite(id: uuid::Uuid, name: &str, slug: &str, logo: &Option<String>) -> Value {
    json!({"name": name, "slug": slug, "id": id, "logo_url": logo})
}

/// Full `WorkSpaceMemberInviteSerializer` (`serializers/workspace.py:117-133`:
/// `__all__` + workspace lite + `invite_link`).
fn ws_invite_json(r: &WsInviteRow) -> Value {
    json!({
        "id": r.id,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "email": r.email,
        "accepted": r.accepted,
        "token": r.token,
        "message": r.message,
        "responded_at": r.responded_at,
        "role": r.role,
        "workspace": ws_lite(r.workspace_id, &r.ws_name, &r.ws_slug, &r.ws_logo),
        "created_by": opt_uuid(&r.created_by_id),
        "updated_by": opt_uuid(&r.updated_by_id),
        "invite_link": workspace_invite_link(&r.id.to_string(), &r.ws_slug, &r.token),
    })
}

/// Public join-GET shape (`WorkSpaceMemberInvitePublicSerializer`,
/// `serializers/workspace.py:140-164`): token + invite_link OMITTED so an
/// unauthenticated caller cannot retrieve the acceptance token
/// (GHSA-86mg-259g-pwgg / GHSA-gf48-p6jp-cwc4); email INCLUDED.
fn ws_invite_public_json(r: &WsInviteRow) -> Value {
    json!({
        "id": r.id,
        "email": r.email,
        "workspace": ws_lite(r.workspace_id, &r.ws_name, &r.ws_slug, &r.ws_logo),
        "role": r.role,
        "message": r.message,
        "accepted": r.accepted,
        "responded_at": r.responded_at,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "created_by": opt_uuid(&r.created_by_id),
    })
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ProjInviteRow {
    id: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    email: String,
    accepted: bool,
    token: String,
    message: Option<String>,
    responded_at: Option<chrono::DateTime<chrono::Utc>>,
    role: i16,
    workspace_id: uuid::Uuid,
    ws_name: String,
    ws_slug: String,
    ws_logo: Option<String>,
    project_id: uuid::Uuid,
    proj_identifier: String,
    proj_name: String,
    proj_cover_image: Option<String>,
    proj_logo_props: Value,
    proj_description: String,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
}

const PROJ_INVITE_COLS: &str = "i.id, i.created_at, i.updated_at, i.email, i.accepted, i.token, \
    i.message, i.responded_at, i.role, i.workspace_id, \
    w.name AS ws_name, w.slug AS ws_slug, w.logo AS ws_logo, \
    p.id AS project_id, p.identifier AS proj_identifier, p.name AS proj_name, \
    p.cover_image AS proj_cover_image, p.logo_props AS proj_logo_props, \
    p.description AS proj_description, i.created_by_id, i.updated_by_id";

fn proj_lite(r: &ProjInviteRow) -> Value {
    json!({"id": r.project_id, "identifier": r.proj_identifier, "name": r.proj_name,
        "cover_image": r.proj_cover_image, "cover_image_url": r.proj_cover_image,
        "logo_props": r.proj_logo_props, "description": r.proj_description})
}

/// Full `ProjectMemberInviteSerializer` (`serializers/project.py`: `__all__`
/// + project/workspace lite).
fn proj_invite_json(r: &ProjInviteRow) -> Value {
    json!({
        "id": r.id,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "email": r.email,
        "accepted": r.accepted,
        "token": r.token,
        "message": r.message,
        "responded_at": r.responded_at,
        "role": r.role,
        "workspace": ws_lite(r.workspace_id, &r.ws_name, &r.ws_slug, &r.ws_logo),
        "project": proj_lite(r),
        "created_by": opt_uuid(&r.created_by_id),
        "updated_by": opt_uuid(&r.updated_by_id),
    })
}

/// Public join-GET shape (`ProjectMemberInvitePublicSerializer`): email +
/// token OMITTED (GHSA-2r58-hgv7-635q).
fn proj_invite_public_json(r: &ProjInviteRow) -> Value {
    json!({
        "id": r.id,
        "project": proj_lite(r),
        "workspace": ws_lite(r.workspace_id, &r.ws_name, &r.ws_slug, &r.ws_logo),
        "role": r.role,
        "message": r.message,
        "accepted": r.accepted,
        "responded_at": r.responded_at,
    })
}

// ============================================================================
// E5c — workspace invitations.
// ============================================================================

/// Mirrors `WorkspaceInvitationsViewset` list (default `list`, gate
/// `WorkSpaceAdminPermission`): GET 200 full-serializer array.
pub async fn ws_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if gate_ws_invite(&st.pool, auth.0, &slug).await?.is_none() {
        let e = deny_detail();
        return Ok(e);
    }
    let rows: Vec<WsInviteRow> = sqlx::query_as(&format!(
        "SELECT {WS_INVITE_COLS} FROM workspace_member_invites i \
         JOIN workspaces w ON w.id = i.workspace_id \
         WHERE w.slug = $1 AND i.deleted_at IS NULL ORDER BY i.created_at DESC"
    ))
    .bind(&slug)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!(rows.iter().map(ws_invite_json).collect::<Vec<_>>())),
    ))
}

/// Parses the `{emails:[{email,role?}]}` bulk body shared by both invite
/// create paths: entries in order; role coerced (default 5). Garbage role →
/// Err (Django `int()` → 500 — sane 400, documented).
fn parse_invite_entries(body: &Value) -> Result<Vec<(Value, InviteEntry)>, String> {
    let arr = body
        .get("emails")
        .and_then(Value::as_array)
        .ok_or_else(|| EMAILS_REQUIRED_MSG.to_string())?;
    if arr.is_empty() {
        return Err(EMAILS_REQUIRED_MSG.to_string());
    }
    let mut out = Vec::with_capacity(arr.len());
    for raw in arr {
        let role = match raw.get("role") {
            None | Some(Value::Null) => 5,
            Some(v) => coerce_role_value(v).map_err(|_| VALID_DETAIL_MSG.to_string())?,
        };
        let email_raw = raw.get("email").and_then(Value::as_str).unwrap_or("");
        out.push((
            raw.clone(),
            InviteEntry { email: normalize_email(email_raw), role },
        ));
    }
    Ok(out)
}

/// Mirrors `WorkspaceInvitationsViewset.create`
/// (`plane/app/views/workspace/invite.py:52-128`): gate 20/15 (DRF 403);
/// empty → 400 (`:55-56`); entry role above requester → 400 (`:62-66`);
/// already-member → 400 + `workspace_users` (`:72-85`); per-email validate
/// (`:89-110`); bulk `ignore_conflicts` (`:112-114`, live unique
/// `(email, workspace_id) WHERE deleted_at IS NULL` — verified `\d`);
/// celery SKIPPED; **200** `{"message":"Emails sent successfully"}` (`:128`).
pub async fn ws_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let Some(requester_role) = gate_ws_invite(&st.pool, auth.0, &slug).await? else {
        let e = deny_detail();
        return Ok(e);
    };
    let entries = match parse_invite_entries(&body) {
        Ok(e) => e,
        Err(m) if m == EMAILS_REQUIRED_MSG => {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": m}))));
        }
        Err(m) => {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": m}))));
        }
    };
    // `invite.py:62-66` — no higher-role invites.
    for (_, e) in &entries {
        if let Err(m) = guard_invite_role(requester_role, e.role) {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": m}))));
        }
    }
    let Some(workspace_id) = ws_id_by_slug(&st.pool, &slug).await? else {
        return Ok(missing());
    };
    // `invite.py:72-85` — already-member check over the RAW emails.
    let raw_emails: Vec<&str> = entries
        .iter()
        .map(|(raw, _)| raw.get("email").and_then(Value::as_str).unwrap_or(""))
        .collect();
    let existing: Vec<WsMemberLiteRow> = sqlx::query_as(
        "SELECT wm.id, wm.member_id, wm.role, u.email AS u_email FROM workspace_members wm \
         JOIN users u ON u.id = wm.member_id \
         WHERE wm.workspace_id = $1 AND u.email = ANY($2) \
         AND wm.is_active = true AND wm.deleted_at IS NULL",
    )
    .bind(workspace_id)
    .bind(&raw_emails)
    .fetch_all(&st.pool)
    .await?;
    if !existing.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": ALREADY_MEMBER_MSG,
                "workspace_users": existing.iter().map(ws_member_lite_json).collect::<Vec<_>>(),
            })),
        ));
    }
    // `invite.py:89-110` — validate + build (invalid → 400 verbatim).
    for (raw, e) in &entries {
        if !is_valid_email(&e.email) || e.email.is_empty() {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": invalid_email_msg(raw)})),
            ));
        }
    }
    // Bulk `ignore_conflicts` (`:112-114`).
    let mut tx = st.pool.begin().await?;
    for (_, e) in &entries {
        let token = mint_invite_token(&e.email, &st.config.jwt_secret);
        sqlx::query(
            "INSERT INTO workspace_member_invites (id, email, role, token, accepted, \
             workspace_id, created_by_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, false, $4, $5, now(), now()) \
             ON CONFLICT DO NOTHING",
        )
        .bind(&e.email)
        .bind(e.role)
        .bind(&token)
        .bind(workspace_id)
        .bind(auth.0)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok((StatusCode::OK, Json(json!({"message": WS_SENT_MSG}))))
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct WsMemberLiteRow {
    id: uuid::Uuid,
    member_id: uuid::Uuid,
    role: i16,
    u_email: Option<String>,
}

/// Minimal `WorkSpaceMemberSerializer` row for the `workspace_users` error
/// payload (`invite.py:82`).
fn ws_member_lite_json(r: &WsMemberLiteRow) -> Value {
    json!({"id": r.id, "member": r.member_id, "role": r.role, "email": r.u_email})
}

async fn fetch_ws_invite(
    pool: &sqlx::PgPool,
    slug: &str,
    pk: uuid::Uuid,
) -> Result<Option<WsInviteRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {WS_INVITE_COLS} FROM workspace_member_invites i \
         JOIN workspaces w ON w.id = i.workspace_id \
         WHERE w.slug = $1 AND i.id = $2 AND i.deleted_at IS NULL"
    ))
    .bind(slug)
    .bind(pk)
    .fetch_optional(pool)
    .await
}

/// Mirrors `WorkspaceInvitationsViewset.retrieve` (default, gate
/// `WorkSpaceAdminPermission`): GET 200 full shape; miss → 404.
pub async fn ws_detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if gate_ws_invite(&st.pool, auth.0, &slug).await?.is_none() {
        let e = deny_detail();
        return Ok(e);
    }
    match fetch_ws_invite(&st.pool, &slug, pk).await? {
        Some(r) => Ok((StatusCode::OK, Json(ws_invite_json(&r)))),
        None => Ok(missing()),
    }
}

/// Mirrors `WorkspaceInvitationsViewset.partial_update` (default, gate
/// `WorkSpaceAdminPermission`): PATCH 200. Writable keys follow the
/// serializer `read_only_fields`
/// (`serializers/workspace.py:127-133` — everything else writable, i.e.
/// `role` + `accepted` in practice); unknown keys ignored (DRF drops
/// undeclared input). Miss → 404.
pub async fn ws_patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if gate_ws_invite(&st.pool, auth.0, &slug).await?.is_none() {
        let e = deny_detail();
        return Ok(e);
    }
    if fetch_ws_invite(&st.pool, &slug, pk).await?.is_none() {
        return Ok(missing());
    }
    let new_role: Option<i16> = match body.get("role") {
        None | Some(Value::Null) => None,
        Some(v) => match coerce_role_value(v) {
            Ok(r) if ROLES.contains(&r) => Some(r),
            _ => {
                let raw = v.to_string();
                let raw = raw.trim_matches('"');
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"role": [format!("\"{raw}\" is not a valid choice.")]})),
                ));
            }
        },
    };
    let new_accepted: Option<bool> = match body.get("accepted") {
        None | Some(Value::Null) => None,
        Some(v) => match parse_drf_bool(v) {
            Some(b) => Some(b),
            None => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"accepted": ["Must be a valid boolean."]})),
                ));
            }
        },
    };
    sqlx::query(
        "UPDATE workspace_member_invites i SET role = COALESCE($1, i.role), \
         accepted = COALESCE($2, i.accepted), updated_at = now() \
         FROM workspaces w WHERE i.id = $3 AND i.workspace_id = w.id AND w.slug = $4",
    )
    .bind(new_role)
    .bind(new_accepted)
    .bind(pk)
    .bind(&slug)
    .execute(&st.pool)
    .await?;
    match fetch_ws_invite(&st.pool, &slug, pk).await? {
        Some(r) => Ok((StatusCode::OK, Json(ws_invite_json(&r)))),
        None => Ok(missing()),
    }
}

/// Mirrors `WorkspaceInvitationsViewset.destroy`
/// (`plane/app/views/workspace/invite.py:130-133`): gate 20/15; HARD
/// `delete()` (no soft override on the model — verified) → **204**;
/// miss → 404.
pub async fn ws_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if gate_ws_invite(&st.pool, auth.0, &slug).await?.is_none() {
        let e = deny_detail();
        return Ok(e);
    }
    let n = sqlx::query(
        "DELETE FROM workspace_member_invites i USING workspaces w \
         WHERE i.id = $1 AND i.workspace_id = w.id AND w.slug = $2",
    )
    .bind(pk)
    .bind(&slug)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

// ============================================================================
// E5d — workspace join.
// ============================================================================

/// Mirrors `WorkspaceJoinEndpoint.get`
/// (`plane/app/views/workspace/invite.py:227-233`): PUBLIC (`AllowAny` —
/// NO auth extractor, NO token/email needed); 200 public shape (email
/// included; token + invite_link omitted); miss → 404.
pub async fn ws_join_get(
    State(st): State<AppState>,
    Path((slug, pk)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    match fetch_ws_invite(&st.pool, &slug, pk).await? {
        Some(r) => Ok((StatusCode::OK, Json(ws_invite_public_json(&r)))),
        None => Ok(missing()),
    }
}

/// Default `WorkspaceMember` props for join-created rows
/// (`get_default_props` ×2 + `get_issue_props`, `db/models/workspace.py`).
fn default_ws_member_props() -> (Value, Value, Value) {
    let props = json!({
        "filters": {"priority": null, "state": null, "state_group": null, "assignees": null,
            "created_by": null, "labels": null, "start_date": null, "target_date": null,
            "subscriber": null},
        "display_filters": {"group_by": null, "order_by": "-created_at", "type": null,
            "sub_issue": true, "show_empty_groups": true, "layout": "list",
            "calendar_date_range": ""},
    });
    let issue_props = json!({"subscribed": true, "assigned": true, "created": true, "all_issues": true});
    (props.clone(), props, issue_props)
}

/// Mirrors `WorkspaceJoinEndpoint.post`
/// (`plane/app/views/workspace/invite.py:149-225`): authed only (the Axum
/// `AuthUser` extractor answers generic 401 first — Django `:165-169`
/// verbatim 401 is subsumed by the locked "401 generic" rule; ordering
/// deviation documented); token mismatch → 403 (`:155-159`); invitee-email
/// mismatch → 403 (`:170-174`); already responded → 400 (`:222-225`);
/// accept → reactivate (`is_active=True, role=invite.role`, `:192-195`) or
/// create (`:197-202`), invite row HARD-DELETED (`:209`), 200
/// `{"message":"Workspace Invitation Accepted"}`; reject → `responded_at`
/// set, row kept, 200 `{"message":"Workspace Invitation was not accepted"}`.
///
/// Documented skips: `user.last_workspace_id` (`:204-206`) — the live
/// `users` table has NO such column (verified via `\d`/information_schema;
/// Django would error there too); celery/cache decorators.
pub async fn ws_join_post(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let Some(invite) = fetch_ws_invite(&st.pool, &slug, pk).await? else {
        return Ok(missing());
    };
    // `invite.py:155-159` — exact token equality (never 401 here).
    let token = body.get("token").and_then(Value::as_str).unwrap_or("");
    if token.is_empty() || token != invite.token {
        return Ok((
            StatusCode::FORBIDDEN,
            Json(json!({"error": WS_JOIN_FORBIDDEN_MSG})),
        ));
    }
    let email_row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT email FROM users WHERE id = $1")
            .bind(auth.0)
            .fetch_optional(&st.pool)
            .await?;
    let user_email = email_row.and_then(|(e,)| e).unwrap_or_default();
    // `invite.py:170-174` — accepter must own the invitee address.
    if user_email.to_lowercase() != invite.email.to_lowercase() {
        return Ok((
            StatusCode::FORBIDDEN,
            Json(json!({"error": INVITE_EMAIL_MISMATCH_MSG})),
        ));
    }
    // `invite.py:177,222-225` — single response only.
    if invite.responded_at.is_some() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": ALREADY_RESPONDED_MSG})),
        ));
    }
    let accepted = ws_accepted_truthy(body.get("accepted"));
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE workspace_member_invites SET accepted = $1, responded_at = now(), updated_at = now() \
         WHERE id = $2",
    )
    .bind(accepted)
    .bind(pk)
    .execute(&mut *tx)
    .await?;
    if accepted {
        // `invite.py:189-202` — reactivate or create (by invite email; the
        // authed user owns it per the check above).
        let member_row: Option<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT id FROM workspace_members WHERE workspace_id = $1 AND member_id = $2",
        )
        .bind(invite.workspace_id)
        .bind(auth.0)
        .fetch_optional(&mut *tx)
        .await?;
        if member_row.is_some() {
            sqlx::query(
                "UPDATE workspace_members SET is_active = true, role = $1, updated_at = now() \
                 WHERE workspace_id = $2 AND member_id = $3",
            )
            .bind(invite.role)
            .bind(invite.workspace_id)
            .bind(auth.0)
            .execute(&mut *tx)
            .await?;
        } else {
            let (view_props, default_props, issue_props) = default_ws_member_props();
            sqlx::query(
                "INSERT INTO workspace_members (id, workspace_id, member_id, role, view_props, \
                 default_props, issue_props, is_active, getting_started_checklist, tips, \
                 explored_features, created_at, updated_at) \
                 VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, true, '{}', '{}', '{}', now(), now()) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(invite.workspace_id)
            .bind(auth.0)
            .bind(invite.role)
            .bind(&view_props)
            .bind(&default_props)
            .bind(&issue_props)
            .execute(&mut *tx)
            .await?;
        }
        // `invite.py:209` — HARD delete on accept (verified, no soft override).
        sqlx::query("DELETE FROM workspace_member_invites WHERE id = $1")
            .bind(pk)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok((StatusCode::OK, Json(json!({"message": WS_ACCEPTED_MSG}))))
    } else {
        tx.commit().await?;
        Ok((StatusCode::OK, Json(json!({"message": WS_REJECTED_MSG}))))
    }
}

// ============================================================================
// E5f — project invitations (INTENDED behavior per locked §13, not Django's
// 3 bugs: (1) `filter(...).role` AttributeError → 500 on ANY non-empty
// create — `project/invite.py:65-67`; (2) the role-mismatch 400 missing
// `status=` so it returns 200 — `:69-70`; (3) `project_invitations.delay`
// called on the LIST — `:107-114` — crashing after the rows are built.
// Rust implements the evident intent: proper 400s, 200 success, no celery.)
// ============================================================================

/// Mirrors `ProjectInvitationsViewset` list (default, `IsAuthenticated`
/// only — no `permission_classes`, no `allow_permission`): GET 200 array.
pub async fn proj_list(
    State(st): State<AppState>,
    _auth: AuthUser,
    Path((slug, project_id)): Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if project_in_workspace(&st.pool, project_id, &slug).await?.is_none() {
        return Ok(missing());
    }
    let rows: Vec<ProjInviteRow> = sqlx::query_as(&format!(
        "SELECT {PROJ_INVITE_COLS} FROM project_member_invites i \
         JOIN workspaces w ON w.id = i.workspace_id \
         JOIN projects p ON p.id = i.project_id \
         WHERE w.slug = $1 AND i.project_id = $2 AND i.deleted_at IS NULL \
         ORDER BY i.created_at DESC"
    ))
    .bind(&slug)
    .bind(project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!(rows.iter().map(proj_invite_json).collect::<Vec<_>>())),
    ))
}

/// Mirrors the INTENDED `ProjectInvitationsViewset.create`
/// (`plane/app/views/project/invite.py:56-116`): gate ADMIN project-level;
/// empty → 400 (`:61-62`); ws role 5/20 invitee with a different role →
/// 400 (`:69-70` — Rust returns the INTENDED 400, Django returns 200 by
/// omitting `status=`); per-email validate (`:76-98`); bulk
/// `ignore_conflicts` (no live unique index — plain inserts); celery
/// SKIPPED; **200** `{"message":"Email sent successfully"}` (`:116`).
pub async fn proj_create(
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
    let entries = match parse_invite_entries(&body) {
        Ok(e) => e,
        Err(m) => {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": m}))));
        }
    };
    // `invite.py:64-70` (intended): invitee ws role 5/20 must equal the
    // requested role (default 5). Lookup by address, active only.
    for (raw, e) in &entries {
        let raw_addr = raw.get("email").and_then(Value::as_str).unwrap_or("");
        let ws_role: Option<i16> = sqlx::query_scalar(
            "SELECT wm.role FROM workspace_members wm JOIN users u ON u.id = wm.member_id \
             WHERE wm.workspace_id = $1 AND u.email = $2 \
             AND wm.is_active = true AND wm.deleted_at IS NULL",
        )
        .bind(workspace_id)
        .bind(raw_addr)
        .fetch_optional(&st.pool)
        .await?;
        if let Some(wr) = ws_role {
            if (wr == 5 || wr == 20) && wr != e.role {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": DIFF_ROLE_MSG})),
                ));
            }
        }
        if !is_valid_email(&e.email) || e.email.is_empty() {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": invalid_email_msg(raw)})),
            ));
        }
    }
    let mut tx = st.pool.begin().await?;
    for (_, e) in &entries {
        let token = mint_invite_token(&e.email, &st.config.jwt_secret);
        sqlx::query(
            "INSERT INTO project_member_invites (id, email, role, token, accepted, \
             project_id, workspace_id, created_by_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, false, $4, $5, $6, now(), now()) \
             ON CONFLICT DO NOTHING",
        )
        .bind(&e.email)
        .bind(e.role)
        .bind(&token)
        .bind(project_id)
        .bind(workspace_id)
        .bind(auth.0)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok((StatusCode::OK, Json(json!({"message": PROJ_SENT_MSG}))))
}

async fn fetch_proj_invite(
    pool: &sqlx::PgPool,
    slug: &str,
    project_id: uuid::Uuid,
    pk: uuid::Uuid,
) -> Result<Option<ProjInviteRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {PROJ_INVITE_COLS} FROM project_member_invites i \
         JOIN workspaces w ON w.id = i.workspace_id \
         JOIN projects p ON p.id = i.project_id \
         WHERE w.slug = $1 AND i.project_id = $2 AND i.id = $3 AND i.deleted_at IS NULL"
    ))
    .bind(slug)
    .bind(project_id)
    .bind(pk)
    .fetch_optional(pool)
    .await
}

/// Mirrors `ProjectInvitationsViewset.retrieve` (default, `IsAuthenticated`
/// only): GET 200 full shape; miss → 404.
pub async fn proj_detail(
    State(st): State<AppState>,
    _auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    match fetch_proj_invite(&st.pool, &slug, project_id, pk).await? {
        Some(r) => Ok((StatusCode::OK, Json(proj_invite_json(&r)))),
        None => Ok(missing()),
    }
}

/// Mirrors `ProjectInvitationsViewset.destroy` (default, `IsAuthenticated`
/// only): HARD `DELETE` (default DRF `destroy` → `instance.delete()`; no
/// soft override exists on `ProjectMemberInvite` — the only `delete`
/// override in the codebase is `Workspace.delete`, `db/models/workspace.py:
/// 156`) → **204**; miss → 404.
pub async fn proj_destroy(
    State(st): State<AppState>,
    _auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let n = sqlx::query(
        "DELETE FROM project_member_invites i USING workspaces w \
         WHERE i.id = $1 AND i.project_id = $2 AND i.workspace_id = w.id AND w.slug = $3",
    )
    .bind(pk)
    .bind(project_id)
    .bind(&slug)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

// ============================================================================
// E5f — project join. (The SECRET-network `Only workspace admins...` check
// lives on join-PROJECTS — `UserProjectInvitationsViewset.create`,
// `project/invite.py:141-146`, already shipped in `users_me::join_projects`
// — NOT on this endpoint, which has no network check in Django.)
// ============================================================================

/// Mirrors `ProjectJoinEndpoint.get`
/// (`plane/app/views/project/invite.py:283-286`): PUBLIC; 200 public shape
/// (NO email/token keys); miss → 404.
pub async fn proj_join_get(
    State(st): State<AppState>,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    match fetch_proj_invite(&st.pool, &slug, project_id, pk).await? {
        Some(r) => Ok((StatusCode::OK, Json(proj_invite_public_json(&r)))),
        None => Ok(missing()),
    }
}

/// Mirrors `ProjectJoinEndpoint.post`
/// (`plane/app/views/project/invite.py:195-281`): authed only (generic 401
/// via extractor, locked rule); token mismatch → 403 (`:201-205`); anon →
/// 401; email mismatch → 403 (`:216-220`); non-bool `accepted` → 400
/// (verbatim, `:224-228`); already responded → 400 (`:278-281`); accept →
/// ws row created with the 15-cap (`:242-246`) or reactivated role-kept
/// (`:247-250`), project row created or reactivated KEEPING its role
/// (`:253-266` — `project_member.role = project_member.role` is a verbatim
/// no-op, replicated), 200 `{"message":"Project Invitation Accepted"}`;
/// reject → 200 `{"message":"Project Invitation was not accepted"}`.
///
/// Documented sanies: `ProjectMember` creation fills `workspace_id` (Django
/// omits it → `IntegrityError` 400 on a NOT NULL column); the project-row
/// lookup filters `workspace_id + member` only (`:253-255`, replicated
/// literally — a member elsewhere in the ws reactivates THAT row); no
/// `ProjectUserProperty` row (Django creates none here, unlike bulk
/// create); the invite row is KEPT with `responded_at` set (Django never
/// deletes it).
pub async fn proj_join_post(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let Some(invite) = fetch_proj_invite(&st.pool, &slug, project_id, pk).await? else {
        return Ok(missing());
    };
    // `invite.py:201-205` — exact token equality (never 401 here).
    let token = body.get("token").and_then(Value::as_str).unwrap_or("");
    if token.is_empty() || token != invite.token {
        return Ok((
            StatusCode::FORBIDDEN,
            Json(json!({"error": PROJ_JOIN_FORBIDDEN_MSG})),
        ));
    }
    let email_row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT email FROM users WHERE id = $1")
            .bind(auth.0)
            .fetch_optional(&st.pool)
            .await?;
    let user_email = email_row.and_then(|(e,)| e).unwrap_or_default();
    // `invite.py:216-220`.
    if user_email.to_lowercase() != invite.email.to_lowercase() {
        return Ok((
            StatusCode::FORBIDDEN,
            Json(json!({"error": INVITE_EMAIL_MISMATCH_MSG})),
        ));
    }
    // `invite.py:224-228` — missing defaults False; non-bool → verbatim 400.
    let accepted = match body.get("accepted") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": ACCEPTED_BOOL_MSG})),
            ));
        }
    };
    // `invite.py:278-281`.
    if invite.responded_at.is_some() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": ALREADY_RESPONDED_MSG})),
        ));
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE project_member_invites SET accepted = $1, responded_at = now(), updated_at = now() \
         WHERE id = $2",
    )
    .bind(accepted)
    .bind(pk)
    .execute(&mut *tx)
    .await?;
    if accepted {
        // `invite.py:239-250` — ws row: create capped, else reactivate kept.
        let ws_row: Option<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT id FROM workspace_members WHERE workspace_id = $1 AND member_id = $2",
        )
        .bind(invite.workspace_id)
        .bind(auth.0)
        .fetch_optional(&mut *tx)
        .await?;
        if ws_row.is_none() {
            let (view_props, default_props, issue_props) = default_ws_member_props();
            sqlx::query(
                "INSERT INTO workspace_members (id, workspace_id, member_id, role, view_props, \
                 default_props, issue_props, is_active, getting_started_checklist, tips, \
                 explored_features, created_at, updated_at) \
                 VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, true, '{}', '{}', '{}', now(), now()) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(invite.workspace_id)
            .bind(auth.0)
            .bind(cap_role_for_ws(invite.role))
            .bind(&view_props)
            .bind(&default_props)
            .bind(&issue_props)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "UPDATE workspace_members SET is_active = true, updated_at = now() \
                 WHERE workspace_id = $1 AND member_id = $2",
            )
            .bind(invite.workspace_id)
            .bind(auth.0)
            .execute(&mut *tx)
            .await?;
        }
        // `invite.py:253-266` — project row lookup is workspace-scoped only
        // (replicated literally); reactivation KEEPS the existing role
        // (`project_member.role = project_member.role`, the verbatim no-op).
        let pm_row: Option<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT id FROM project_members WHERE workspace_id = $1 AND member_id = $2 \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(invite.workspace_id)
        .bind(auth.0)
        .fetch_optional(&mut *tx)
        .await?;
        if pm_row.is_none() {
            sqlx::query(
                "INSERT INTO project_members (id, project_id, member_id, role, workspace_id, \
                 created_by_id, view_props, default_props, preferences, sort_order, is_active, \
                 created_at, updated_at) \
                 VALUES (gen_random_uuid(), $1, $2, $3, $4, $2, '{}', '{}', '{}', 65535, true, now(), now()) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(project_id)
            .bind(auth.0)
            .bind(invite.role)
            .bind(invite.workspace_id)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "UPDATE project_members SET is_active = true, updated_at = now() WHERE id = $1",
            )
            .bind(pm_row.map(|(id,)| id).expect("row checked above"))
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok((StatusCode::OK, Json(json!({"message": PROJ_ACCEPTED_MSG}))))
    } else {
        tx.commit().await?;
        Ok((StatusCode::OK, Json(json!({"message": PROJ_REJECTED_MSG}))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_normalize_strips_and_lowers() {
        // `workspace/invite.py:93` / `project/invite.py:80`.
        assert_eq!(normalize_email("  A@B.CoM "), "a@b.com");
    }

    #[test]
    fn invalid_email_msg_shape() {
        // `workspace/invite.py:107` — `Invalid email - <obj> provided ...`.
        let m = invalid_email_msg(&json!({"email": "bad", "role": 5}));
        assert!(m.starts_with("Invalid email - "));
        assert!(m.ends_with(" provided a valid email address is required to send the invite"));
        assert!(m.contains("'email': 'bad'"));
    }

    #[test]
    fn invite_role_gate() {
        // `workspace/invite.py:62-66` — verbatim 400 above requester.
        assert_eq!(
            guard_invite_role(15, 20).unwrap_err(),
            "You cannot invite a user with higher role"
        );
        assert!(guard_invite_role(20, 20).is_ok());
        assert!(guard_invite_role(5, 5).is_ok());
    }

    #[test]
    fn token_round_trip_no_expiry() {
        // Scheme `workspace/invite.py:95-99`: HS256, `{email,timestamp}`,
        // verifiable with the JWT infra, NO expiry claim.
        let secret = "test-secret-for-invite-tokens";
        let tok = mint_invite_token("a@b.com", secret);
        assert_eq!(verify_invite_token(&tok, secret).unwrap(), "a@b.com");
        assert!(verify_invite_token(&tok, "wrong-secret").is_err());
        // Django's `jwt.encode` default is three-segment compact JWT.
        assert_eq!(tok.split('.').count(), 3);
    }

    #[test]
    fn ws_accepted_truthiness_matrix() {
        // `workspace/invite.py:178` — Python truthiness, NO bool check.
        assert!(!ws_accepted_truthy(None));
        assert!(!ws_accepted_truthy(Some(&json!(null))));
        assert!(!ws_accepted_truthy(Some(&json!(false))));
        assert!(ws_accepted_truthy(Some(&json!(true))));
        // Python truthiness quirks: non-empty string (even "false") accepts.
        assert!(ws_accepted_truthy(Some(&json!("false"))));
        assert!(!ws_accepted_truthy(Some(&json!(""))));
        assert!(ws_accepted_truthy(Some(&json!(1))));
        assert!(!ws_accepted_truthy(Some(&json!(0))));
    }

    #[test]
    fn ws_role_cap() {
        // `project/invite.py:245`.
        assert_eq!(cap_role_for_ws(20), 15);
        assert_eq!(cap_role_for_ws(15), 15);
        assert_eq!(cap_role_for_ws(5), 5);
    }

    #[test]
    fn entries_require_emails_key() {
        // `workspace/invite.py:55-56` / `project/invite.py:61-62`.
        assert_eq!(
            parse_invite_entries(&json!({})).unwrap_err(),
            "Emails are required"
        );
        assert_eq!(
            parse_invite_entries(&json!({"emails": []})).unwrap_err(),
            "Emails are required"
        );
        let entries =
            parse_invite_entries(&json!({"emails": [{"email": " A@X.io ", "role": 15}]})).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1.email, "a@x.io");
        assert_eq!(entries[0].1.role, 15);
    }

    #[test]
    fn handler_symbols_wired() {
        // Wiring guards: every E5c/E5d/E5f handler exists for `main.rs`.
        let _ = super::ws_list;
        let _ = super::ws_create;
        let _ = super::ws_detail;
        let _ = super::ws_patch;
        let _ = super::ws_destroy;
        let _ = super::ws_join_get;
        let _ = super::ws_join_post;
        let _ = super::proj_list;
        let _ = super::proj_create;
        let _ = super::proj_detail;
        let _ = super::proj_destroy;
        let _ = super::proj_join_get;
        let _ = super::proj_join_post;
    }
}
