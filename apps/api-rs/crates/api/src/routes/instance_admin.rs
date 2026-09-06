// crates/api/src/routes/instance_admin.rs
//
// E1 — instance-admin god-mode (JWT) paritas Django.
//
// Django sources (all paths under `apps/api/plane/`):
// - `license/api/views/admin.py` (InstanceAdminEndpoint post/get/delete,
//   InstanceAdminSignUpEndpoint, InstanceAdminSignInEndpoint,
//   InstanceAdminUserMeEndpoint, InstanceAdminSignOutEndpoint)
// - `license/api/views/instance.py` (InstanceEndpoint.patch)
// - `license/api/views/configuration.py` (InstanceConfigurationEndpoint,
//   DisableEmailFeatureEndpoint, EmailCredentialCheckEndpoint)
// - `license/api/views/workspace.py` (InstanceWorkSpaceEndpoint,
//   InstanceWorkSpaceAvailabilityCheckEndpoint)
// - `license/api/permissions/instance.py` (InstanceAdminPermission)
// - `license/utils/encryption.py` (Fernet encrypt/decrypt)
// - `license/utils/instance_value.py` (SKIP_ENV_VAR config source switch)
//
// Locked E1 decisions mirrored here:
// - JSON auth (no 302 anywhere; FE handles redirects) with
//   `{"error_code","error_message"}` bodies (codes from
//   `authentication/adapter/error.py:62-71`).
// - Statuses: 5175 → 401, 5190 → 403, every other admin code → 400
//   (Django used 302 redirects, so there is no Django status to copy;
//   the mapping mirrors severity).
// - Gate: `InstanceAdmin(role >= 15)`; deny is the DRF permission-class
//   body 403 `{"detail": "You do not have permission to perform this
//   action."` (NOT the app-level `deny()` message).
// - Multi-write paths run in one tx; soft-delete via UPDATE except the
//   Django hard-delete (`admins/:pk/` DELETE is idempotent 204).
// - Celery side-effects SKIPPED everywhere.
// - OUT (deliberately not wired): `admins/session/`,
//   `sign-up-screen-visited/`, `changelog/`.

use axum::{
    extract::{ConnectInfo, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::{middleware::auth::AuthUser, state::AppState};
use common::auth as authn;

use super::issue_common::{
    next_cursor_str, page_window, parse_cursor, parse_python_int, prev_cursor_str, total_pages,
    DetailEnvelope, PageWindow,
};
use super::project::missing;

// ============================================================================
// Error codes + messages — quoted from Django with file:line.
// ============================================================================

/// `authentication/adapter/error.py:6` (global instance code).
pub const INSTANCE_NOT_CONFIGURED_CODE: i32 = 5000;
/// `authentication/adapter/error.py:63`.
pub const ADMIN_ALREADY_EXIST_CODE: i32 = 5150;
/// `authentication/adapter/error.py:64`.
pub const REQUIRED_ADMIN_EMAIL_PASSWORD_FIRST_NAME_CODE: i32 = 5155;
/// `authentication/adapter/error.py:65`.
pub const INVALID_ADMIN_EMAIL_CODE: i32 = 5160;
/// `authentication/adapter/error.py:67`.
pub const REQUIRED_ADMIN_EMAIL_PASSWORD_CODE: i32 = 5170;
/// `authentication/adapter/error.py:68`.
pub const ADMIN_AUTHENTICATION_FAILED_CODE: i32 = 5175;
/// `authentication/adapter/error.py:69`.
pub const ADMIN_USER_ALREADY_EXIST_CODE: i32 = 5180;
/// `authentication/adapter/error.py:70`.
pub const ADMIN_USER_DOES_NOT_EXIST_CODE: i32 = 5185;
/// `authentication/adapter/error.py:71`.
pub const ADMIN_USER_DEACTIVATED_CODE: i32 = 5190;
/// `authentication/adapter/error.py:17` (zxcvbn score < 3 on sign-up).
pub const PASSWORD_TOO_WEAK_CODE: i32 = 5021;

/// `license/api/views/admin.py:55` (admins POST without email).
pub const EMAIL_REQUIRED_MSG: &str = "Email is required";
/// `license/api/views/admin.py:60,76` (admins POST/GET without an instance).
pub const INSTANCE_NOT_REGISTERED_MSG: &str = "Instance is not registered yet";
/// `license/api/views/base.py:92-96` (IntegrityError → dup admin).
pub const PAYLOAD_INVALID_MSG: &str = "The payload is not valid";
/// DRF permission-class deny body (mirrors `cycle.rs:PERMISSION_DETAIL_MSG`).
pub const PERMISSION_DETAIL_MSG: &str = "You do not have permission to perform this action.";
/// `license/api/views/base.py:104-108` (ValidationError shape for PATCH).
pub const VALID_DETAIL_MSG: &str = "Please provide valid detail";
/// `license/api/views/configuration.py:93` (missing receiver_email).
pub const RECEIVER_REQUIRED_MSG: &str = "Receiver email is required";
/// `license/api/views/configuration.py:129` (SMTP success).
pub const EMAIL_SENT_MSG: &str = "Email successfully sent.";
/// `license/api/views/configuration.py:83` (disable-email failure).
pub const DISABLE_EMAIL_FAILED_MSG: &str = "Failed to disable email configuration";
/// `license/api/views/workspace.py:80` (workspace create name+slug).
pub const NAME_SLUG_REQUIRED_MSG: &str = "Both name and slug are required";
/// `license/api/views/workspace.py:86` (workspace create lengths).
pub const NAME_SLUG_LENGTH_MSG: &str = "The maximum length for name is 80 and for slug is 48";
/// `license/api/serializers/workspace.py:40` (restricted slug).
pub const SLUG_NOT_VALID_MSG: &str = "Slug is not valid";
/// `license/api/serializers/workspace.py:43` (taken slug, pre-check).
pub const SLUG_IN_USE_MSG: &str = "Slug is already in use";
/// `license/api/views/workspace.py:108` (race dup slug — note the `slug` key).
pub const SLUG_EXISTS_MSG: &str = "The workspace with the slug already exists";
/// `license/api/serializers/workspace.py:27` (name contains URL).
pub const NAME_URL_MSG: &str = "Name must not contain URLs";
/// `license/api/serializers/workspace.py:33` (symbol-only name).
pub const NAME_ALNUM_MSG: &str = "Name must contain at least one letter or number";
/// DRF `SlugField` default (`rest_framework/fields.py`) — the license
/// `WorkspaceSerializer` uses `fields="__all__"`, so malformed slugs 400
/// with this message before the restricted/taken checks run.
pub const SLUG_FORMAT_MSG: &str =
    "Enter a valid \"slug\" consisting of letters, numbers, underscores or hyphens.";
/// `license/api/views/workspace.py:27` (slug-check without slug).
pub const SLUG_CHECK_REQUIRED_MSG: &str = "Workspace Slug is required";
/// `auth/sign-out` parity message for the JSON sign-out below.
pub const LOGGED_OUT_MSG: &str = "Logged out";

/// Locked status mapping: 5175 → 401, 5190 → 403, everything else → 400.
pub fn admin_error_status(code: i32) -> StatusCode {
    match code {
        ADMIN_AUTHENTICATION_FAILED_CODE => StatusCode::UNAUTHORIZED,
        ADMIN_USER_DEACTIVATED_CODE => StatusCode::FORBIDDEN,
        _ => StatusCode::BAD_REQUEST,
    }
}

fn auth_error(code: i32, message: &str) -> Json<Value> {
    Json(json!({"error_code": code, "error_message": message}))
}

fn deny_detail() -> (StatusCode, Json<Value>) {
    (
        StatusCode::FORBIDDEN,
        Json(json!({"detail": PERMISSION_DETAIL_MSG})),
    )
}

fn is_constraint_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.code().is_some_and(|c| c.starts_with("23")))
}

/// Client IP ala Django `get_client_ip` (`utils/ip_address.py:199-205`):
/// first X-Forwarded-For entry, else peer socket, else `0.0.0.0`
/// (E11 precedent, `auth.rs:sign_out`).
pub fn client_ip_from(headers: &HeaderMap, addr: Option<std::net::SocketAddr>) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|xff| xff.split(',').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| addr.map(|a| a.ip().to_string()))
        .unwrap_or_else(|| "0.0.0.0".to_string())
}

fn user_agent_of(headers: &HeaderMap) -> String {
    headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}

/// Raw refresh cookie (`plane_rt` / `__Host-plane_rt`), mirroring the
/// private reader in `auth.rs` (kept local: `main.rs`-only modification
/// rule forbids touching `auth.rs` for a shared helper).
fn refresh_cookie_raw(headers: &HeaderMap) -> Option<String> {
    let cookies = headers.get("cookie")?.to_str().ok()?;
    for pair in cookies.split(';') {
        let (k, v) = pair.trim().split_once('=')?;
        if k == "plane_rt" || k == "__Host-plane_rt" {
            return Some(v.trim().to_string());
        }
    }
    None
}

// ============================================================================
// Gate — `license/api/permissions/instance.py:12-18`.
// ============================================================================

/// Pure decision bit: the role threshold (`role__gte=15`).
pub fn gate_role_allowed(role: Option<i16>) -> bool {
    role.map(|r| r >= 15).unwrap_or(false)
}

async fn instance_id(pool: &PgPool) -> Result<Option<uuid::Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM instances WHERE deleted_at IS NULL ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
}

/// `InstanceAdminPermission.has_permission`: anonymous → False; otherwise
/// `InstanceAdmin(role>=15, instance, user).exists()`. No instance row → False
/// (Django filters `instance=None`, which matches nothing).
async fn instance_admin_allowed(pool: &PgPool, uid: uuid::Uuid) -> Result<bool, sqlx::Error> {
    let Some(iid) = instance_id(pool).await? else {
        return Ok(false);
    };
    let role: Option<i16> = sqlx::query_scalar(
        "SELECT role FROM instance_admins WHERE instance_id = $1 AND user_id = $2 AND deleted_at IS NULL",
    )
    .bind(iid)
    .bind(uid)
    .fetch_optional(pool)
    .await?;
    Ok(gate_role_allowed(role))
}

// ============================================================================
// Session issuance — same JWT pair as app login (`auth.rs:login`).
// ============================================================================

const ACCESS_TTL_SECS: i64 = 900;
const REFRESH_TTL_SECS: i64 = 30 * 24 * 3600;

fn session_cookie_headers(access: &str, refresh: &str, secure: bool) -> HeaderMap {
    let (at_name, rt_name) = if secure {
        ("__Host-plane_at", "__Host-plane_rt")
    } else {
        ("plane_at", "plane_rt")
    };
    let mut headers = HeaderMap::new();
    for h in [
        authn::cookie_headers(at_name, access, ACCESS_TTL_SECS, secure),
        authn::cookie_headers(rt_name, refresh, REFRESH_TTL_SECS, secure),
    ] {
        if let Ok(v) = h.parse() {
            headers.append("set-cookie", v);
        }
    }
    headers
}

async fn issue_admin_session(
    st: &AppState,
    uid: &uuid::Uuid,
) -> Result<HeaderMap, common::errors::AppError> {
    let access = authn::encode_access(uid, &st.config.jwt_secret, ACCESS_TTL_SECS);
    let (hash_rt, raw_rt) = authn::new_refresh();
    let family = uuid::Uuid::new_v4().to_string();
    let mut conn = st.redis_client().await.map_err(|e| {
        tracing::warn!(error=%e, "instance-admin: redis unavailable");
        common::errors::AppError::internal()
    })?;
    redis::cmd("SET")
        .arg(authn::refresh_key(&hash_rt))
        .arg(format!("{uid}:{family}"))
        .arg("EX")
        .arg(REFRESH_TTL_SECS)
        .query_async::<()>(&mut conn)
        .await
        .map_err(|e| {
            tracing::warn!(error=%e, "instance-admin: refresh store failed");
            common::errors::AppError::internal()
        })?;
    redis::cmd("SADD")
        .arg(format!("auth:family:{family}"))
        .arg(&hash_rt)
        .query_async::<()>(&mut conn)
        .await
        .map_err(|e| {
            tracing::warn!(error=%e, "instance-admin: family store failed");
            common::errors::AppError::internal()
        })?;
    redis::cmd("EXPIRE")
        .arg(format!("auth:family:{family}"))
        .arg(REFRESH_TTL_SECS)
        .query_async::<()>(&mut conn)
        .await
        .map_err(|e| {
            tracing::warn!(error=%e, "instance-admin: family expire failed");
            common::errors::AppError::internal()
        })?;
    Ok(session_cookie_headers(
        &access,
        &raw_rt,
        st.config.cookie_secure,
    ))
}

// ============================================================================
// Admin-me / user-detail JSON shapes.
// ============================================================================

#[derive(Debug, Clone, sqlx::FromRow)]
struct AdminMeRow {
    id: uuid::Uuid,
    avatar: String,
    avatar_url: Option<String>,
    cover_image: Option<String>,
    date_joined: chrono::DateTime<chrono::Utc>,
    display_name: String,
    email: Option<String>,
    first_name: String,
    last_name: String,
    is_active: bool,
    is_bot: bool,
    is_email_verified: bool,
    user_timezone: String,
    username: String,
    is_password_autoset: bool,
}

async fn fetch_admin_me(pool: &PgPool, uid: uuid::Uuid) -> Result<Option<AdminMeRow>, sqlx::Error> {
    sqlx::query_as::<_, AdminMeRow>(
        "SELECT u.id, u.avatar, COALESCE(fa.asset, NULLIF(u.avatar, '')) AS avatar_url, \
                u.cover_image, u.date_joined, u.display_name, u.email, u.first_name, u.last_name, \
                u.is_active, u.is_bot, u.is_email_verified, u.user_timezone, u.username, \
                u.is_password_autoset \
         FROM users u LEFT JOIN file_assets fa ON fa.id = u.avatar_asset_id \
         WHERE u.id = $1",
    )
    .bind(uid)
    .fetch_optional(pool)
    .await
}

/// `license/api/serializers/admin.py:12-33` (`InstanceAdminMeSerializer`).
fn admin_me_json(r: &AdminMeRow) -> Value {
    json!({
        "id": r.id,
        "avatar": r.avatar,
        "avatar_url": r.avatar_url,
        "cover_image": r.cover_image,
        "date_joined": r.date_joined,
        "display_name": r.display_name,
        "email": r.email,
        "first_name": r.first_name,
        "last_name": r.last_name,
        "is_active": r.is_active,
        "is_bot": r.is_bot,
        "is_email_verified": r.is_email_verified,
        "user_timezone": r.user_timezone,
        "username": r.username,
        "is_password_autoset": r.is_password_autoset,
    })
}

// ============================================================================
// E1a — sign-in / sign-up / sign-out.
// ============================================================================

#[derive(Debug, Deserialize, Default)]
pub struct AdminSignInBody {
    pub email: Option<String>,
    pub password: Option<String>,
}

/// POST /api/instances/admins/sign-in/ — JSON twin of
/// `InstanceAdminSignInEndpoint.post` (`admin.py:269-404`): same check order
/// (instance → required → email-valid → user-exists → bot → active →
/// password → is-admin), same stamps, but 200 + JWT cookies + admin-me body
/// instead of a 302, and `{"error_code","error_message"}` instead of a
/// redirect query string.
pub async fn sign_in(
    State(st): State<AppState>,
    addr: Option<ConnectInfo<std::net::SocketAddr>>,
    headers: HeaderMap,
    Json(body): Json<AdminSignInBody>,
) -> Result<(StatusCode, HeaderMap, Json<Value>), common::errors::AppError> {
    let err = |code: i32, msg: &str| {
        let status = admin_error_status(code);
        Ok((status, HeaderMap::new(), auth_error(code, msg)))
    };
    let iid = instance_id(&st.pool).await?;
    if iid.is_none() {
        return err(INSTANCE_NOT_CONFIGURED_CODE, "INSTANCE_NOT_CONFIGURED");
    }
    let email = body.email.unwrap_or_default().trim().to_lowercase();
    let password = body.password.unwrap_or_default();
    // `admin.py:292` (`if not email or not password`).
    if email.is_empty() || password.is_empty() {
        return err(
            REQUIRED_ADMIN_EMAIL_PASSWORD_CODE,
            "REQUIRED_ADMIN_EMAIL_PASSWORD",
        );
    }
    // `admin.py:305-318` (validate_email).
    if !crate::routes::auth::email_valid(&email) {
        return err(INVALID_ADMIN_EMAIL_CODE, "INVALID_ADMIN_EMAIL");
    }
    let row: Option<(uuid::Uuid, String, bool, bool)> =
        sqlx::query_as("SELECT id, password, is_bot, is_active FROM users WHERE email = $1")
            .bind(&email)
            .fetch_optional(&st.pool)
            .await?;
    // `admin.py:321-334` (no such user).
    let Some((uid, hash, is_bot, is_active)) = row else {
        return err(ADMIN_USER_DOES_NOT_EXIST_CODE, "ADMIN_USER_DOES_NOT_EXIST");
    };
    // `admin.py:336-353` (bot guard reuses ADMIN_AUTHENTICATION_FAILED).
    if is_bot {
        return err(
            ADMIN_AUTHENTICATION_FAILED_CODE,
            "ADMIN_AUTHENTICATION_FAILED",
        );
    }
    // `admin.py:355-365` (deactivated).
    if !is_active {
        return err(ADMIN_USER_DEACTIVATED_CODE, "ADMIN_USER_DEACTIVATED");
    }
    // `admin.py:367-378` (bad password).
    if !authn::verify_django_password(&password, &hash) {
        return err(
            ADMIN_AUTHENTICATION_FAILED_CODE,
            "ADMIN_AUTHENTICATION_FAILED",
        );
    }
    // `admin.py:380-391` (not an instance admin).
    let iid = iid.expect("checked above");
    let is_admin: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM instance_admins WHERE instance_id = $1 AND user_id = $2 AND deleted_at IS NULL)",
    )
    .bind(iid)
    .bind(uid)
    .fetch_one(&st.pool)
    .await?;
    if !is_admin {
        return err(
            ADMIN_AUTHENTICATION_FAILED_CODE,
            "ADMIN_AUTHENTICATION_FAILED",
        );
    }
    // `admin.py:392-399` (stamps; `token_updated_at` rotates `token` on
    // Django `User.save` — mirrored by writing both columns here).
    let ip = client_ip_from(&headers, addr.map(|a| a.0));
    let ua = user_agent_of(&headers);
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    sqlx::query(
        "UPDATE users SET is_active = true, last_active = now(), last_login_time = now(), \
         last_login_ip = $1, last_login_uagent = $2, token_updated_at = now(), token = $3, \
         updated_at = now() WHERE id = $4",
    )
    .bind(&ip)
    .bind(&ua)
    .bind(&token)
    .bind(uid)
    .execute(&st.pool)
    .await?;
    let cookies = issue_admin_session(&st, &uid).await?;
    let Some(me) = fetch_admin_me(&st.pool, uid).await? else {
        return Ok((StatusCode::NOT_FOUND, HeaderMap::new(), missing().1));
    };
    Ok((StatusCode::OK, cookies, Json(admin_me_json(&me))))
}

#[derive(Debug, Deserialize, Default)]
pub struct AdminSignUpBody {
    pub email: Option<String>,
    pub password: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub company_name: Option<String>,
    pub is_telemetry_enabled: Option<Value>,
}

/// Parses `is_telemetry_enabled` (`admin.py:129` took the raw form string):
/// JSON bool preferred; `"True"/"False"/"1"/"0"` (any case) honoured;
/// missing → true.
pub fn parse_telemetry(raw: Option<&Value>) -> bool {
    match raw {
        None => true,
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => !matches!(
            s.trim().to_lowercase().as_str(),
            "false" | "0" | "no" | "off"
        ),
        Some(Value::Number(n)) => n.as_i64().unwrap_or(1) != 0,
        _ => true,
    }
}

/// POST /api/instances/admins/sign-up/ — JSON twin of
/// `InstanceAdminSignUpEndpoint.post` (`admin.py:90-266`): same pre-checks +
/// row-locked (`SELECT … FOR UPDATE`) check-and-create in one tx (user with
/// `username=uuid4hex` + pbkdf2 hash, profile, stamps, instance_admin row,
/// `is_setup_done`), then 200 + JWT like sign-in (Django 302'd to
/// `general/`).
pub async fn sign_up(
    State(st): State<AppState>,
    addr: Option<ConnectInfo<std::net::SocketAddr>>,
    headers: HeaderMap,
    Json(body): Json<AdminSignUpBody>,
) -> Result<(StatusCode, HeaderMap, Json<Value>), common::errors::AppError> {
    let err = |code: i32, msg: &str| {
        let status = admin_error_status(code);
        Ok((status, HeaderMap::new(), auth_error(code, msg)))
    };
    // `admin.py:96-106` (no instance row yet).
    let inst: Option<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT id FROM instances WHERE deleted_at IS NULL ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_optional(&st.pool)
    .await?;
    let Some((iid,)) = inst else {
        return err(INSTANCE_NOT_CONFIGURED_CODE, "INSTANCE_NOT_CONFIGURED");
    };
    // `admin.py:108-121` (fast pre-check; authoritative re-check in tx).
    let setup_done: bool = sqlx::query_scalar(
        "SELECT COALESCE((SELECT is_setup_done FROM instances WHERE id = $1), false)",
    )
    .bind(iid)
    .fetch_one(&st.pool)
    .await?;
    let admin_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM instance_admins WHERE deleted_at IS NULL)")
            .fetch_one(&st.pool)
            .await?;
    if setup_done || admin_exists {
        return err(ADMIN_ALREADY_EXIST_CODE, "ADMIN_ALREADY_EXIST");
    }
    let email = body.email.unwrap_or_default().trim().to_lowercase();
    let password = body.password.unwrap_or_default();
    let first_name = body.first_name.unwrap_or_default();
    let last_name = body.last_name.unwrap_or_default();
    let company_name = body.company_name.unwrap_or_default();
    let telemetry = parse_telemetry(body.is_telemetry_enabled.as_ref());
    // `admin.py:131-151` (email+password+first_name required).
    if email.is_empty() || password.is_empty() || first_name.trim().is_empty() {
        return err(
            REQUIRED_ADMIN_EMAIL_PASSWORD_FIRST_NAME_CODE,
            "REQUIRED_ADMIN_EMAIL_PASSWORD_FIRST_NAME",
        );
    }
    // `admin.py:153-173` (validate_email).
    if !crate::routes::auth::email_valid(&email) {
        return err(INVALID_ADMIN_EMAIL_CODE, "INVALID_ADMIN_EMAIL");
    }
    // `admin.py:177-193` (email already registered).
    let taken: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)")
        .bind(&email)
        .fetch_one(&st.pool)
        .await?;
    if taken {
        return err(ADMIN_USER_ALREADY_EXIST_CODE, "ADMIN_USER_ALREADY_EXIST");
    }
    // `admin.py:195-212` (zxcvbn score < 3; reused helper from E8,
    // `auth_compat.rs:password_strong_enough`).
    if !crate::routes::auth_compat::password_strong_enough(&password) {
        return err(PASSWORD_TOO_WEAK_CODE, "PASSWORD_TOO_WEAK");
    }
    let ip = client_ip_from(&headers, addr.map(|a| a.0));
    let ua = user_agent_of(&headers);
    // `admin.py:214-260` (atomic check-and-create under a row lock).
    let mut tx = st.pool.begin().await?;
    let locked: Option<(uuid::Uuid, bool)> =
        sqlx::query_as("SELECT id, is_setup_done FROM instances WHERE id = $1 FOR UPDATE")
            .bind(iid)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((iid, locked_done)) = locked else {
        tx.rollback().await?;
        return err(INSTANCE_NOT_CONFIGURED_CODE, "INSTANCE_NOT_CONFIGURED");
    };
    let locked_admin: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM instance_admins WHERE deleted_at IS NULL)")
            .fetch_one(&mut *tx)
            .await?;
    if locked_done || locked_admin {
        tx.rollback().await?;
        return err(ADMIN_ALREADY_EXIST_CODE, "ADMIN_ALREADY_EXIST");
    }
    // `admin.py:236-243` (`username=uuid4().hex`, `make_password`).
    let username = uuid::Uuid::new_v4().simple().to_string();
    let hash = authn::make_django_password(&password);
    let display_name = email.split('@').next().unwrap_or(&email).to_string();
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let (uid,): (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO users (id, email, username, password, first_name, last_name, display_name, \
         avatar, date_joined, token, user_timezone, last_location, created_location, \
         last_login_ip, last_logout_ip, last_login_medium, last_login_uagent, last_active, \
         last_login_time, token_updated_at, is_active, is_staff, is_superuser, is_managed, \
         is_password_expired, is_email_verified, is_password_autoset, is_bot, is_email_valid, \
         is_password_reset_required, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, '', now(), $7, 'UTC', '', '', \
         $8, '', 'email', $9, now(), now(), now(), true, false, false, false, false, false, \
         false, false, true, false, now(), now()) RETURNING id",
    )
    .bind(&email)
    .bind(&username)
    .bind(&hash)
    .bind(first_name.trim())
    .bind(&last_name)
    .bind(&display_name)
    .bind(&token)
    .bind(&ip)
    .bind(&ua)
    .fetch_one(&mut *tx)
    .await?;
    // `admin.py:244` (`Profile(user, company_name)`; every NOT NULL profile
    // column spelled out with its Django model default — `user.py:25-49`
    // for the JSON blobs, `"INDIA"`/`"en"`/`"full"`, `get_random_color`
    // → random hex here).
    let bg = format!("#{:06X}", rand::random::<u32>() & 0xFFFFFF);
    sqlx::query(
        "INSERT INTO profiles (id, user_id, theme, is_tour_completed, onboarding_step, \
         is_onboarded, billing_address_country, has_billing_address, company_name, \
         is_mobile_onboarded, mobile_onboarding_step, mobile_timezone_auto_set, language, \
         is_smooth_cursor_enabled, start_of_the_week, is_app_rail_docked, background_color, \
         goals, has_marketing_email_consent, is_navigation_tour_completed, \
         is_subscribed_to_changelog, notification_view_mode, product_tour, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, '{}', false, $2, false, 'INDIA', false, $3, false, $4, \
         false, 'en', false, 0, true, $5, '{}', false, false, false, 'full', $6, now(), now())",
    )
    .bind(uid)
    .bind(json!({"profile_complete": false, "workspace_create": false, "workspace_invite": false, "workspace_join": false}))
    .bind(&company_name)
    .bind(json!({"profile_complete": false, "workspace_create": false, "workspace_join": false}))
    .bind(&bg)
    .bind(json!({"work_items": false, "cycles": false, "modules": false, "intake": false, "pages": false}))
    .execute(&mut *tx)
    .await?;
    // `admin.py:254-260` (admin row + setup flag; default role 20).
    sqlx::query(
        "INSERT INTO instance_admins (id, instance_id, user_id, role, is_verified, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, 20, false, now(), now())",
    )
    .bind(iid)
    .bind(uid)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE instances SET is_setup_done = true, instance_name = $1, \
         is_telemetry_enabled = $2, updated_at = now() WHERE id = $3",
    )
    .bind(&company_name)
    .bind(telemetry)
    .bind(iid)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let cookies = issue_admin_session(&st, &uid).await?;
    let Some(me) = fetch_admin_me(&st.pool, uid).await? else {
        return Ok((StatusCode::NOT_FOUND, HeaderMap::new(), missing().1));
    };
    Ok((StatusCode::OK, cookies, Json(admin_me_json(&me))))
}

/// POST /api/instances/admins/sign-out/ — JSON twin of
/// `InstanceAdminSignOutEndpoint.post` (`admin.py:428-444`): stamps
/// `last_logout_ip/time`, revokes the refresh family best-effort, then
/// **200** + cleared cookies (reused E8 helper — NO 302; the FE redirects).
pub async fn sign_out(
    State(st): State<AppState>,
    auth: AuthUser,
    addr: Option<ConnectInfo<std::net::SocketAddr>>,
    headers: HeaderMap,
) -> Result<(StatusCode, HeaderMap, Json<Value>), common::errors::AppError> {
    let ip = client_ip_from(&headers, addr.map(|a| a.0));
    if !instance_admin_allowed(&st.pool, auth.0).await? {
        return Ok((
            StatusCode::FORBIDDEN,
            crate::routes::user::cleared_cookie_headers(st.config.cookie_secure),
            Json(json!({"detail": PERMISSION_DETAIL_MSG})),
        ));
    }
    let _ = sqlx::query(
        "UPDATE users SET last_logout_ip = $1, last_logout_time = now(), updated_at = now() WHERE id = $2",
    )
    .bind(&ip)
    .bind(auth.0)
    .execute(&st.pool)
    .await;
    if let Some(raw) = refresh_cookie_raw(&headers) {
        if let Ok(mut conn) = st.redis_client().await {
            let hash = authn::sha256hex(raw.trim());
            let val: Option<String> = redis::cmd("GET")
                .arg(authn::refresh_key(&hash))
                .query_async(&mut conn)
                .await
                .unwrap_or(None);
            let _: redis::RedisResult<()> = redis::cmd("DEL")
                .arg(authn::refresh_key(&hash))
                .query_async(&mut conn)
                .await;
            if let Some(val) = val {
                if let Some((_, family)) = val.split_once(':') {
                    let _: redis::RedisResult<()> = redis::cmd("SREM")
                        .arg(format!("auth:family:{family}"))
                        .arg(&hash)
                        .query_async(&mut conn)
                        .await;
                }
            }
        }
    }
    let headers = crate::routes::user::cleared_cookie_headers(st.config.cookie_secure);
    Ok((
        StatusCode::OK,
        headers,
        Json(json!({"message": LOGGED_OUT_MSG})),
    ))
}

// ============================================================================
// E1b — admins list/create/delete/me.
// ============================================================================

#[derive(Debug, Clone, sqlx::FromRow)]
struct AdminListRow {
    id: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    role: i32,
    is_verified: bool,
    instance_id: uuid::Uuid,
    user_id: Option<uuid::Uuid>,
    first_name: Option<String>,
    last_name: Option<String>,
    avatar: Option<String>,
    avatar_url: Option<String>,
    is_bot: Option<bool>,
    display_name: Option<String>,
    email: Option<String>,
    last_login_medium: Option<String>,
}

const ADMIN_LIST_SELECT: &str = "ia.id, ia.created_at, ia.updated_at, ia.role, ia.is_verified, \
    ia.instance_id, ia.user_id, u.first_name, u.last_name, u.avatar, \
    COALESCE(fa.asset, NULLIF(u.avatar, '')) AS avatar_url, u.is_bot, u.display_name, \
    u.email, u.last_login_medium";

/// `license/api/serializers/admin.py:36-42` (`InstanceAdminSerializer`:
/// `__all__` + nested `user_detail`).
fn admin_json(r: &AdminListRow) -> Value {
    json!({
        "id": r.id,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "role": r.role,
        "is_verified": r.is_verified,
        "instance": r.instance_id,
        "user": r.user_id,
        "user_detail": {
            "id": r.user_id,
            "first_name": r.first_name,
            "last_name": r.last_name,
            "avatar": r.avatar,
            "avatar_url": r.avatar_url,
            "is_bot": r.is_bot,
            "display_name": r.display_name,
            "email": r.email,
            "last_login_medium": r.last_login_medium,
        },
    })
}

async fn fetch_admin_row(
    pool: &PgPool,
    admin_id: uuid::Uuid,
) -> Result<Option<AdminListRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {ADMIN_LIST_SELECT} FROM instance_admins ia \
         LEFT JOIN users u ON u.id = ia.user_id \
         LEFT JOIN file_assets fa ON fa.id = u.avatar_asset_id \
         WHERE ia.id = $1 AND ia.deleted_at IS NULL"
    );
    sqlx::query_as::<_, AdminListRow>(&sql)
        .bind(admin_id)
        .fetch_optional(pool)
        .await
}

/// GET /api/instances/admins/ — `InstanceAdminEndpoint.get`
/// (`admin.py:71-81`): 200 list, no pagination; no instance → 403 verbatim.
pub async fn admins_list(
    State(st): State<AppState>,
    auth: AuthUser,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !instance_admin_allowed(&st.pool, auth.0).await? {
        return Ok(deny_detail());
    }
    let iid = instance_id(&st.pool).await?;
    // Gate passed ⇒ instance exists (the gate checks the singleton row).
    let iid = iid.expect("gate implies instance");
    let sql = format!(
        "SELECT {ADMIN_LIST_SELECT} FROM instance_admins ia \
         LEFT JOIN users u ON u.id = ia.user_id \
         LEFT JOIN file_assets fa ON fa.id = u.avatar_asset_id \
         WHERE ia.instance_id = $1 AND ia.deleted_at IS NULL ORDER BY ia.created_at DESC"
    );
    let rows: Vec<AdminListRow> = sqlx::query_as(&sql).bind(iid).fetch_all(&st.pool).await?;
    Ok((
        StatusCode::OK,
        Json(json!(rows.iter().map(admin_json).collect::<Vec<_>>())),
    ))
}

#[derive(Debug, Deserialize, Default)]
pub struct AdminCreateBody {
    pub email: Option<String>,
    pub role: Option<i32>,
}

/// POST /api/instances/admins/ — `InstanceAdminEndpoint.post`
/// (`admin.py:48-69`): **201**; no email → 400 verbatim; no instance → 403
/// verbatim; unknown email → 404 `missing()` (Django `User.DoesNotExist` →
/// `base.py:HandleException` 404); dup → 400 payload-invalid (Django
/// `IntegrityError` → `base.py:92-96`).
pub async fn admins_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<AdminCreateBody>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !instance_admin_allowed(&st.pool, auth.0).await? {
        return Ok(deny_detail());
    }
    let email = body.email.unwrap_or_default().trim().to_lowercase();
    // `admin.py:54-55`.
    if email.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": EMAIL_REQUIRED_MSG})),
        ));
    }
    // `admin.py:57-62`.
    let Some(iid) = instance_id(&st.pool).await? else {
        return Ok((
            StatusCode::FORBIDDEN,
            Json(json!({"error": INSTANCE_NOT_REGISTERED_MSG})),
        ));
    };
    // `admin.py:65` (`.get` miss → 404, not 500 — locked sane mapping).
    let user: Option<(uuid::Uuid,)> = sqlx::query_as("SELECT id FROM users WHERE email = $1")
        .bind(&email)
        .fetch_optional(&st.pool)
        .await?;
    let Some((uid,)) = user else {
        return Ok(missing());
    };
    let role = body.role.unwrap_or(20);
    // `admin.py:67` in a tx so a mid-flight failure rolls back.
    let mut tx = st.pool.begin().await?;
    let admin_id: Result<Option<(uuid::Uuid,)>, sqlx::Error> = sqlx::query_as(
        "INSERT INTO instance_admins (id, instance_id, user_id, role, is_verified, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, false, now(), now()) RETURNING id",
    )
    .bind(iid)
    .bind(uid)
    .bind(role)
    .fetch_optional(&mut *tx)
    .await;
    let admin_id = match admin_id {
        Ok(v) => v,
        Err(e) if is_constraint_violation(&e) => {
            tx.rollback().await?;
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": PAYLOAD_INVALID_MSG})),
            ));
        }
        Err(e) => return Err(e.into()),
    };
    tx.commit().await?;
    let Some((admin_id,)) = admin_id else {
        return Ok(missing());
    };
    match fetch_admin_row(&st.pool, admin_id).await? {
        Some(row) => Ok((StatusCode::CREATED, Json(admin_json(&row)))),
        None => Ok(missing()),
    }
}

/// DELETE /api/instances/admins/:pk/ — `InstanceAdminEndpoint.delete`
/// (`admin.py:83-87`): **204** even when nothing matches (Django
/// `filter().delete()` never raises).
pub async fn admins_delete(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(pk): axum::extract::Path<uuid::Uuid>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !instance_admin_allowed(&st.pool, auth.0).await? {
        return Ok(deny_detail());
    }
    if let Some(iid) = instance_id(&st.pool).await? {
        sqlx::query("DELETE FROM instance_admins WHERE instance_id = $1 AND id = $2")
            .bind(iid)
            .bind(pk)
            .execute(&st.pool)
            .await?;
    }
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

/// Locked: NO `GET .../admins/:pk/` route in Django — fall through to the
/// 404 fallback body (wired explicitly as GET on the `:pk` path because
/// Axum would otherwise answer 405 for the DELETE-only route).
pub async fn admin_pk_get_404() -> (StatusCode, Json<Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(json!({"error": "Page not found."})),
    )
}

/// GET /api/instances/admins/me/ — `InstanceAdminUserMeEndpoint.get`
/// (`admin.py:407-412`): 200 with the `admin.py:12-33` me keys.
pub async fn admins_me(
    State(st): State<AppState>,
    auth: AuthUser,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !instance_admin_allowed(&st.pool, auth.0).await? {
        return Ok(deny_detail());
    }
    match fetch_admin_me(&st.pool, auth.0).await? {
        Some(me) => Ok((StatusCode::OK, Json(admin_me_json(&me)))),
        None => Ok(missing()),
    }
}

// ============================================================================
// E1c — instance PATCH + configurations GET/PATCH.
// ============================================================================

/// `license/api/serializers/instance.py:11-17`: everything writable minus
/// the read-only `id, email, last_checked_at, is_setup_done` (plus the
/// auto-managed audit columns, which DRF also treats as read-only).
#[derive(Debug, Clone, PartialEq)]
pub enum InstancePatchVal {
    Text(Option<String>),
    Flag(bool),
}

pub fn parse_instance_patch_value(key: &str, value: &Value) -> Option<InstancePatchVal> {
    match key {
        "instance_name" | "whitelist_emails" | "instance_id" | "current_version"
        | "latest_version" | "edition" | "domain" | "namespace" => {
            if value.is_null() {
                Some(InstancePatchVal::Text(None))
            } else if let Some(s) = value.as_str() {
                Some(InstancePatchVal::Text(Some(s.to_string())))
            } else if value.is_number() || value.is_boolean() {
                Some(InstancePatchVal::Text(Some(
                    value.to_string().trim_matches('"').to_string(),
                )))
            } else {
                None
            }
        }
        "is_telemetry_enabled"
        | "is_support_required"
        | "is_signup_screen_visited"
        | "is_verified"
        | "is_test"
        | "is_current_version_deprecated" => {
            if let Some(b) = value.as_bool() {
                Some(InstancePatchVal::Flag(b))
            } else if let Some(s) = value.as_str() {
                match s.trim().to_lowercase().as_str() {
                    "1" | "true" | "yes" | "on" => Some(InstancePatchVal::Flag(true)),
                    "0" | "false" | "no" | "off" => Some(InstancePatchVal::Flag(false)),
                    _ => None,
                }
            } else if let Some(n) = value.as_i64() {
                Some(InstancePatchVal::Flag(n != 0))
            } else {
                None
            }
        }
        _ => None,
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct InstancePatchRow {
    id: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    instance_name: String,
    whitelist_emails: Option<String>,
    instance_id: String,
    current_version: String,
    latest_version: Option<String>,
    edition: String,
    domain: String,
    last_checked_at: chrono::DateTime<chrono::Utc>,
    namespace: Option<String>,
    is_telemetry_enabled: bool,
    is_support_required: bool,
    is_setup_done: bool,
    is_signup_screen_visited: bool,
    is_verified: bool,
    is_test: bool,
    is_current_version_deprecated: bool,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
}

fn instance_patch_json(r: &InstancePatchRow) -> Value {
    json!({
        "id": r.id,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "instance_name": r.instance_name,
        "whitelist_emails": r.whitelist_emails,
        "instance_id": r.instance_id,
        "current_version": r.current_version,
        "latest_version": r.latest_version,
        "edition": r.edition,
        "domain": r.domain,
        "last_checked_at": r.last_checked_at,
        "namespace": r.namespace,
        "is_telemetry_enabled": r.is_telemetry_enabled,
        "is_support_required": r.is_support_required,
        "is_setup_done": r.is_setup_done,
        "is_signup_screen_visited": r.is_signup_screen_visited,
        "is_verified": r.is_verified,
        "is_test": r.is_test,
        "is_current_version_deprecated": r.is_current_version_deprecated,
        "created_by": r.created_by_id,
        "updated_by": r.updated_by_id,
    })
}

const INSTANCE_PATCH_SELECT: &str = "id, created_at, updated_at, instance_name, whitelist_emails, \
    instance_id, current_version, latest_version, edition, domain, last_checked_at, namespace, \
    is_telemetry_enabled, is_support_required, is_setup_done, is_signup_screen_visited, \
    is_verified, is_test, is_current_version_deprecated, created_by_id, updated_by_id";

/// PATCH /api/instances/ — `InstanceEndpoint.patch` (`instance.py:161-169`):
/// 200; read-only keys ignored; zero rows → 404 (locked plan decision —
/// Django would crash building a serializer over `None`).
pub async fn instance_patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !instance_admin_allowed(&st.pool, auth.0).await? {
        return Ok(deny_detail());
    }
    let Some(iid) = instance_id(&st.pool).await? else {
        return Ok(missing());
    };
    let obj = body.as_object().cloned().unwrap_or_default();
    let mut texts: Vec<Option<String>> = Vec::new();
    let mut flags: Vec<bool> = Vec::new();
    for (key, value) in &obj {
        match parse_instance_patch_value(key, value) {
            Some(InstancePatchVal::Text(v)) => {
                texts.push(v);
            }
            Some(InstancePatchVal::Flag(v)) => {
                flags.push(v);
            }
            None => {
                // Read-only (`id,email,last_checked_at,is_setup_done`,
                // audit columns) or unknown keys are ignored; a KNOWN key
                // with an unparseable type is a validation 400.
                if is_instance_writable_key(key) {
                    return Ok((
                        StatusCode::BAD_REQUEST,
                        Json(json!({"detail": VALID_DETAIL_MSG})),
                    ));
                }
            }
        }
    }
    if !texts.is_empty() || !flags.is_empty() {
        // Two homogeneous binds would need dynamic typing; instead apply
        // text and flag sets as two statements in one tx (same atomicity).
        let mut tx = st.pool.begin().await?;
        if !texts.is_empty() {
            // Rebuild per-kind SET lists with dense placeholders.
            let text_keys: Vec<&str> = obj
                .iter()
                .filter(|(k, v)| {
                    matches!(
                        parse_instance_patch_value(k, v),
                        Some(InstancePatchVal::Text(_))
                    )
                })
                .map(|(k, _)| k.as_str())
                .collect();
            let set_sql = text_keys
                .iter()
                .enumerate()
                .map(|(i, k)| format!("{k} = ${}", i + 1))
                .collect::<Vec<_>>()
                .join(", ");
            let sql_text = format!(
                "UPDATE instances SET {set_sql}, updated_at = now() WHERE id = ${}",
                text_keys.len() + 1
            );
            let mut q = sqlx::query::<sqlx::Postgres>(&sql_text);
            for v in &texts {
                q = q.bind(v);
            }
            q = q.bind(iid);
            q.execute(&mut *tx).await?;
        }
        if !flags.is_empty() {
            let flag_keys: Vec<&str> = obj
                .iter()
                .filter(|(k, v)| {
                    matches!(
                        parse_instance_patch_value(k, v),
                        Some(InstancePatchVal::Flag(_))
                    )
                })
                .map(|(k, _)| k.as_str())
                .collect();
            let set_sql = flag_keys
                .iter()
                .enumerate()
                .map(|(i, k)| format!("{k} = ${}", i + 1))
                .collect::<Vec<_>>()
                .join(", ");
            let sql_text = format!(
                "UPDATE instances SET {set_sql}, updated_at = now() WHERE id = ${}",
                flag_keys.len() + 1
            );
            let mut q = sqlx::query(&sql_text);
            for v in &flags {
                q = q.bind(v);
            }
            q = q.bind(iid);
            q.execute(&mut *tx).await?;
        }
        tx.commit().await?;
    }
    let sql = format!("SELECT {INSTANCE_PATCH_SELECT} FROM instances WHERE id = $1");
    match sqlx::query_as::<_, InstancePatchRow>(&sql)
        .bind(iid)
        .fetch_optional(&st.pool)
        .await?
    {
        Some(row) => Ok((StatusCode::OK, Json(instance_patch_json(&row)))),
        None => Ok(missing()),
    }
}

fn is_instance_writable_key(key: &str) -> bool {
    matches!(
        key,
        "instance_name"
            | "whitelist_emails"
            | "instance_id"
            | "current_version"
            | "latest_version"
            | "edition"
            | "domain"
            | "namespace"
            | "is_telemetry_enabled"
            | "is_support_required"
            | "is_signup_screen_visited"
            | "is_verified"
            | "is_test"
            | "is_current_version_deprecated"
    )
}

// ---------------------------------------------------------------------------
// Fernet — byte-exact mirror of `license/utils/encryption.py:13-44`.
// ---------------------------------------------------------------------------

/// `encryption.py:13-16`: PBKDF2-HMAC-SHA256(SECRET_KEY, salt=b"salt", 100000).
fn derive_fernet_key(secret: &str) -> [u8; 32] {
    use pbkdf2::pbkdf2_hmac;
    use sha2::Sha256;
    let mut dk = [0u8; 32];
    pbkdf2_hmac::<Sha256>(secret.as_bytes(), b"salt", 100_000, &mut dk);
    dk
}

fn fernet_secret() -> String {
    // Django `settings.SECRET_KEY` (`settings/common.py:32` — env
    // `SECRET_KEY`). Must match Django's value or stored secrets won't
    // decrypt (documented wiring requirement, not a fallback).
    std::env::var("SECRET_KEY").unwrap_or_default()
}

fn pkcs7_pad(data: &[u8]) -> Vec<u8> {
    let pad = 16 - (data.len() % 16);
    let mut out = Vec::with_capacity(data.len() + pad);
    out.extend_from_slice(data);
    out.extend(std::iter::repeat(pad as u8).take(pad));
    out
}

fn pkcs7_unpad(data: &[u8]) -> Option<Vec<u8>> {
    let &last = data.last()?;
    if last == 0 || last > 16 || data.len() < last as usize {
        return None;
    }
    if !data[data.len() - last as usize..]
        .iter()
        .all(|&b| b == last)
    {
        return None;
    }
    Some(data[..data.len() - last as usize].to_vec())
}

fn aes128_cbc_encrypt(key: &[u8; 16], iv: &[u8; 16], plaintext: &[u8]) -> Vec<u8> {
    use aes::cipher::{BlockEncryptMut, KeyInit};
    use aes::Aes128;
    let mut cipher = Aes128::new_from_slice(key).expect("16-byte key");
    let padded = pkcs7_pad(plaintext);
    let mut out = Vec::with_capacity(padded.len());
    let mut prev = *iv;
    for block in padded.chunks(16) {
        let mut buf = [0u8; 16];
        for (i, b) in block.iter().enumerate() {
            buf[i] = b ^ prev[i];
        }
        let mut g = aes::cipher::generic_array::GenericArray::from(buf);
        cipher.encrypt_block_mut(&mut g);
        out.extend_from_slice(&g);
        prev.copy_from_slice(&g);
    }
    out
}

fn aes128_cbc_decrypt(key: &[u8; 16], iv: &[u8; 16], ciphertext: &[u8]) -> Option<Vec<u8>> {
    use aes::cipher::{BlockDecryptMut, KeyInit};
    use aes::Aes128;
    if ciphertext.is_empty() || ciphertext.len() % 16 != 0 {
        return None;
    }
    let mut cipher = Aes128::new_from_slice(key).expect("16-byte key");
    let mut out = Vec::with_capacity(ciphertext.len());
    let mut prev = *iv;
    for block in ciphertext.chunks(16) {
        let mut g = aes::cipher::generic_array::GenericArray::from_slice(block).clone();
        cipher.decrypt_block_mut(&mut g);
        let mut plain = [0u8; 16];
        for (i, b) in g.iter().enumerate() {
            plain[i] = b ^ prev[i];
        }
        out.extend_from_slice(&plain);
        prev.copy_from_slice(block);
    }
    pkcs7_unpad(&out)
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("hmac key");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// `encryption.py:20-27`: Fernet token; empty input → `""`.
pub fn encrypt_data(data: &str, secret: &str) -> String {
    use base64::Engine;
    if data.is_empty() {
        return String::new();
    }
    let key = derive_fernet_key(secret);
    let (signing, encryption): ([u8; 16], [u8; 16]) = (
        key[..16].try_into().expect("split"),
        key[16..].try_into().expect("split"),
    );
    let mut iv = [0u8; 16];
    for b in iv.iter_mut() {
        *b = rand::random::<u8>();
    }
    let now = chrono::Utc::now().timestamp() as u64;
    let ct = aes128_cbc_encrypt(&encryption, &iv, data.as_bytes());
    let mut payload = Vec::with_capacity(1 + 8 + 16 + ct.len() + 32);
    payload.push(0x80);
    payload.extend_from_slice(&now.to_be_bytes());
    payload.extend_from_slice(&iv);
    payload.extend_from_slice(&ct);
    let sig = hmac_sha256(&signing, &payload);
    payload.extend_from_slice(&sig);
    base64::engine::general_purpose::URL_SAFE.encode(&payload)
}

/// `encryption.py:34-44`: decrypt; empty input → `""`; ANY failure → `""`
/// (Django logs + returns `""`).
pub fn decrypt_data(enc: &str, secret: &str) -> String {
    use base64::Engine;
    if enc.is_empty() {
        return String::new();
    }
    let raw = match base64::engine::general_purpose::URL_SAFE.decode(enc.trim()) {
        Ok(r) => r,
        Err(_) => return String::new(),
    };
    if raw.len() < 1 + 8 + 16 + 16 + 32 || raw[0] != 0x80 {
        return String::new();
    }
    let key = derive_fernet_key(secret);
    let (signing, encryption): ([u8; 16], [u8; 16]) = (
        key[..16].try_into().expect("split"),
        key[16..].try_into().expect("split"),
    );
    let (payload, sig) = raw.split_at(raw.len() - 32);
    if !ct_eq(&hmac_sha256(&signing, payload), sig) {
        return String::new();
    }
    let iv: [u8; 16] = payload[9..25].try_into().expect("iv slice");
    let ct = &payload[25..];
    match aes128_cbc_decrypt(&encryption, &iv, ct) {
        Some(pt) => String::from_utf8(pt).unwrap_or_default(),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Configurations.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
struct ConfigRow {
    id: uuid::Uuid,
    key: String,
    value: Option<String>,
    category: String,
    is_encrypted: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

/// `license/api/serializers/configuration.py:15-21` (decrypt on read).
fn config_json(r: &ConfigRow, secret: &str) -> Value {
    let value = match (&r.value, r.is_encrypted) {
        (Some(v), true) => decrypt_data(v, secret),
        (v, _) => v.clone().unwrap_or_default(),
    };
    json!({
        "id": r.id,
        "key": r.key,
        "value": value,
        "category": r.category,
        "is_encrypted": r.is_encrypted,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
    })
}

/// GET /api/instances/configurations/ — `InstanceConfigurationEndpoint.get`
/// (`configuration.py:35-39`): 200 full list, secrets decrypted.
pub async fn configs_list(
    State(st): State<AppState>,
    auth: AuthUser,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !instance_admin_allowed(&st.pool, auth.0).await? {
        return Ok(deny_detail());
    }
    let rows: Vec<ConfigRow> = sqlx::query_as(
        "SELECT id, key, value, category, is_encrypted, created_at, updated_at \
         FROM instance_configurations WHERE deleted_at IS NULL ORDER BY created_at DESC",
    )
    .fetch_all(&st.pool)
    .await?;
    let secret = fernet_secret();
    Ok((
        StatusCode::OK,
        Json(json!(rows
            .iter()
            .map(|r| config_json(r, &secret))
            .collect::<Vec<_>>())),
    ))
}

/// PATCH /api/instances/configurations/ — `InstanceConfigurationEndpoint.patch`
/// (`configuration.py:41-59`): dict-merge over KNOWN keys only (unknown
/// ignored), `None → ""`, strip, encrypt-if-flagged; returns the
/// updated-only array.
pub async fn configs_patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !instance_admin_allowed(&st.pool, auth.0).await? {
        return Ok(deny_detail());
    }
    let obj = body.as_object().cloned().unwrap_or_default();
    if obj.is_empty() {
        return Ok((StatusCode::OK, Json(json!([]))));
    }
    let keys: Vec<String> = obj.keys().cloned().collect();
    let rows: Vec<ConfigRow> = sqlx::query_as(
        "SELECT id, key, value, category, is_encrypted, created_at, updated_at \
         FROM instance_configurations WHERE key = ANY($1) AND deleted_at IS NULL",
    )
    .bind(&keys)
    .fetch_all(&st.pool)
    .await?;
    let secret = fernet_secret();
    let mut tx = st.pool.begin().await?;
    for row in &rows {
        // `configuration.py:49` (`"" if raw is None else str(raw).strip()`).
        let raw = obj.get(&row.key);
        let value = match raw {
            None | Some(Value::Null) => String::new(),
            Some(Value::String(s)) => s.trim().to_string(),
            Some(v) => v.to_string().trim_matches('"').to_string(),
        };
        let stored = if row.is_encrypted {
            encrypt_data(&value, &secret)
        } else {
            value
        };
        sqlx::query(
            "UPDATE instance_configurations SET value = $1, updated_at = now() WHERE id = $2",
        )
        .bind(&stored)
        .bind(row.id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    let ids: Vec<uuid::Uuid> = rows.iter().map(|r| r.id).collect();
    let updated: Vec<ConfigRow> = sqlx::query_as(
        "SELECT id, key, value, category, is_encrypted, created_at, updated_at \
         FROM instance_configurations WHERE id = ANY($1)",
    )
    .bind(&ids)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!(updated
            .iter()
            .map(|r| config_json(r, &secret))
            .collect::<Vec<_>>())),
    ))
}

/// Body wrapper so the failure branch carries
/// `{"error": "Failed to disable email configuration"}` verbatim
/// (`configuration.py:82-85`) while success stays bodyless (Django
/// `Response(status=200)`).
pub async fn disable_email_with_body(
    State(st): State<AppState>,
    auth: AuthUser,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !instance_admin_allowed(&st.pool, auth.0).await? {
        return Ok(deny_detail());
    }
    match sqlx::query(
        "UPDATE instance_configurations SET value = CASE WHEN key = 'ENABLE_SMTP' THEN '0' ELSE '' END, \
         updated_at = now() WHERE key IN ('EMAIL_HOST', 'EMAIL_HOST_USER', 'EMAIL_HOST_PASSWORD', \
         'ENABLE_SMTP', 'EMAIL_PORT', 'EMAIL_FROM') AND deleted_at IS NULL",
    )
    .execute(&st.pool)
    .await
    {
        Ok(_) => Ok((StatusCode::OK, Json(Value::Null))),
        Err(e) => {
            tracing::warn!(error=%e, "disable-email: update failed");
            Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": DISABLE_EMAIL_FAILED_MSG})),
            ))
        }
    }
}

// ============================================================================
// E1d — live SMTP check via lettre.
// ============================================================================

/// `settings/common.py:365`: `SKIP_ENV_VAR = env("SKIP_ENV_VAR","1")=="1"`.
/// True (default) → read from the DB (`instance_value.py:19-33`, decrypting
/// secrets); False → read env directly (`instance_value.py:34-38`).
pub fn skip_env_vars() -> bool {
    std::env::var("SKIP_ENV_VAR")
        .map(|v| v == "1")
        .unwrap_or(true)
}

/// `license/utils/instance_value.py:42-59` defaults for the email keys.
pub struct EmailConfig {
    pub host: String,
    pub user: String,
    pub password: String,
    pub port: String,
    pub use_tls: String,
    pub use_ssl: String,
    pub from: String,
}

pub fn email_config_defaults() -> EmailConfig {
    EmailConfig {
        host: std::env::var("EMAIL_HOST").unwrap_or_default(),
        user: std::env::var("EMAIL_HOST_USER").unwrap_or_default(),
        password: std::env::var("EMAIL_HOST_PASSWORD").unwrap_or_default(),
        port: std::env::var("EMAIL_PORT").unwrap_or_else(|_| "587".to_string()),
        use_tls: std::env::var("EMAIL_USE_TLS").unwrap_or_else(|_| "1".to_string()),
        use_ssl: std::env::var("EMAIL_USE_SSL").unwrap_or_else(|_| "0".to_string()),
        from: std::env::var("EMAIL_FROM")
            .unwrap_or_else(|_| "Team Plane <team@mailer.plane.so>".to_string()),
    }
}

async fn load_email_config(pool: &PgPool) -> EmailConfig {
    let mut cfg = email_config_defaults();
    if !skip_env_vars() {
        return cfg;
    }
    let Ok(rows): Result<Vec<(String, Option<String>, bool)>, _> = sqlx::query_as(
        "SELECT key, value, is_encrypted FROM instance_configurations \
         WHERE key IN ('EMAIL_HOST','EMAIL_HOST_USER','EMAIL_HOST_PASSWORD','EMAIL_PORT', \
         'EMAIL_USE_TLS','EMAIL_USE_SSL','EMAIL_FROM') AND deleted_at IS NULL",
    )
    .fetch_all(pool)
    .await
    else {
        return cfg;
    };
    let secret = fernet_secret();
    let get = |key: &str| -> Option<String> {
        rows.iter().find(|(k, _, _)| k == key).map(|(_, v, enc)| {
            let raw = v.clone().unwrap_or_default();
            if *enc {
                decrypt_data(&raw, &secret)
            } else {
                raw
            }
        })
    };
    // Django falls back to the env default per missing key
    // (`instance_value.py:31-33`).
    if let Some(v) = get("EMAIL_HOST") {
        cfg.host = v;
    }
    if let Some(v) = get("EMAIL_HOST_USER") {
        cfg.user = v;
    }
    if let Some(v) = get("EMAIL_HOST_PASSWORD") {
        cfg.password = v;
    }
    if let Some(v) = get("EMAIL_PORT") {
        cfg.port = v;
    }
    if let Some(v) = get("EMAIL_USE_TLS") {
        cfg.use_tls = v;
    }
    if let Some(v) = get("EMAIL_USE_SSL") {
        cfg.use_ssl = v;
    }
    if let Some(v) = get("EMAIL_FROM") {
        cfg.from = v;
    }
    cfg
}

/// Pure decision bit for the permanent-5xx branch: SMTP 535 (authentication
/// rejected) → invalid-credentials; a message naming the sender → from
/// address invalid; any other 5xx → recipients refused.
pub fn smtp_body_for_permanent(code_5xx: bool, is_535: bool, display: &str) -> Value {
    if is_535 {
        return json!({"error": "Invalid credentials provided"});
    }
    let msg = display.to_lowercase();
    if msg.contains("sender") || msg.contains("from address") || msg.contains("mail from") {
        json!({"error": "From address is invalid."})
    } else if code_5xx {
        json!({"error": "All recipient addresses were refused."})
    } else {
        json!({"error": "Could not send email. Please check your configuration"})
    }
}

/// Pure decision bit for io-backed failures:TimedOut → timeout body,
/// unreachable-network kinds → network body, anything else → None (the
/// caller falls through to "Could not connect …").
pub fn smtp_body_for_io(kind: std::io::ErrorKind) -> Option<Value> {
    use std::io::ErrorKind;
    match kind {
        ErrorKind::TimedOut => {
            Some(json!({"error": "Timeout error while trying to connect to the SMTP server."}))
        }
        ErrorKind::HostUnreachable | ErrorKind::NetworkUnreachable | ErrorKind::NotConnected => {
            Some(
                json!({"error": "Network connection error. Please check your internet connection."}),
            )
        }
        _ => None,
    }
}

/// Maps a lettre SMTP failure onto the Django 400 bodies
/// (`configuration.py:130-171`), verbatim. lettre 0.11 reports failures
/// through predicates (`is_timeout/is_client/is_permanent/is_transient/…`)
/// plus the SMTP `status()` code, so:
/// - timeout (socket or the 30s overall budget) → TimeoutError body;
/// - client/build error → BadHeader body;
/// - permanent 535 → SMTPAuthenticationError body; other 5xx split
///   sender-vs-recipient by message (see `smtp_body_for_permanent`);
/// - connection/TLS/network leftovers → SMTPConnectError body, with the
///   io-kind refinement above (ConnectionError body for unreachable nets);
/// - transient 4xx and anything unclassified → the generic Exception body.
pub fn smtp_error_body(err: &lettre::transport::smtp::Error) -> Value {
    use std::io::ErrorKind;
    const CONNECT: &str = "Could not connect with the SMTP server.";
    const GENERIC: &str = "Could not send email. Please check your configuration";
    if err.is_timeout() {
        return json!({"error": "Timeout error while trying to connect to the SMTP server."});
    }
    if err.is_client() {
        return json!({"error": "Invalid email header."});
    }
    if err.is_permanent() {
        let code = err.status();
        let is_535 = code
            .map(|c| format!("{c}").starts_with("535"))
            .unwrap_or(false);
        return smtp_body_for_permanent(true, is_535, &format!("{err}"));
    }
    if err.is_transient() {
        return json!({"error": GENERIC});
    }
    let mut current: Option<&dyn std::error::Error> = Some(err);
    while let Some(e) = current {
        if let Some(io) = e.downcast_ref::<std::io::Error>() {
            if io.kind() == ErrorKind::TimedOut {
                return json!({"error": "Timeout error while trying to connect to the SMTP server."});
            }
            if let Some(body) = smtp_body_for_io(io.kind()) {
                return body;
            }
            break;
        }
        current = e.source();
    }
    json!({"error": CONNECT})
}

#[derive(Debug, Deserialize, Default)]
pub struct EmailCheckBody {
    pub receiver_email: Option<String>,
}

/// POST /api/instances/email-credentials-check/ —
/// `EmailCredentialCheckEndpoint.post` (`configuration.py:88-171`): live
/// send (subject/body verbatim `:117-118`), outcomes mapped to the Django
/// 400 bodies; success 200 `{"message":"Email successfully sent."}`.
/// `port=int(...)` failure → 400 generic (Django would 500 — sane mapping).
/// The send runs in `spawn_blocking` (lettre transport is blocking) under a
/// 30s timeout (→ the Django `TimeoutError` body).
pub async fn email_check(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<EmailCheckBody>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !instance_admin_allowed(&st.pool, auth.0).await? {
        return Ok(deny_detail());
    }
    let to = body.receiver_email.unwrap_or_default().trim().to_string();
    // `configuration.py:91-95`.
    if to.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": RECEIVER_REQUIRED_MSG})),
        ));
    }
    let generic = || {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Could not send email. Please check your configuration"})),
        )
    };
    let cfg = load_email_config(&st.pool).await;
    let port: u16 = match cfg.port.trim().parse::<i64>() {
        Ok(p) if (1..=65535).contains(&p) => p as u16,
        _ => return Ok(generic()),
    };
    use lettre::transport::smtp::authentication::{Credentials, Mechanism};
    use lettre::transport::smtp::client::{Tls, TlsParameters};
    use lettre::{Message, SmtpTransport, Transport};
    // `configuration.py:113-114` (`use_tls == "1"`, `use_ssl == "1"`).
    let tls = if cfg.use_ssl.trim() == "1" {
        match TlsParameters::new(cfg.host.clone()) {
            Ok(p) => Tls::Wrapper(p),
            Err(_) => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "Could not connect with the SMTP server."})),
                ));
            }
        }
    } else if cfg.use_tls.trim() == "1" {
        match TlsParameters::new(cfg.host.clone()) {
            Ok(p) => Tls::Required(p),
            Err(_) => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "Could not connect with the SMTP server."})),
                ));
            }
        }
    } else {
        Tls::None
    };
    let mut builder = SmtpTransport::builder_dangerous(cfg.host.clone())
        .port(port)
        .tls(tls)
        .timeout(Some(std::time::Duration::from_secs(15)));
    if !cfg.user.trim().is_empty() {
        builder = builder
            .credentials(Credentials::new(cfg.user.clone(), cfg.password.clone()))
            .authentication(vec![Mechanism::Plain]);
    }
    let transport = builder.build();
    let from: lettre::message::Mailbox = match cfg.from.trim().parse() {
        Ok(m) => m,
        Err(_) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "From address is invalid."})),
            ));
        }
    };
    let to_box: lettre::message::Mailbox = match to.parse() {
        Ok(m) => m,
        Err(_) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "All recipient addresses were refused."})),
            ));
        }
    };
    // `configuration.py:117-118` subject/body verbatim.
    let msg = match Message::builder()
        .from(from)
        .to(to_box)
        .subject("Email Notification from Plane")
        .body("This is a sample email notification sent from Plane application.".to_string())
    {
        Ok(m) => m,
        Err(_) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "Invalid email header."})),
            ));
        }
    };
    let sent = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        tokio::task::spawn_blocking(move || transport.send(&msg)).await
    })
    .await;
    match sent {
        Err(_) => Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Timeout error while trying to connect to the SMTP server."})),
        )),
        Ok(Err(_)) => Ok(generic()),
        Ok(Ok(Err(e))) => Ok((StatusCode::BAD_REQUEST, Json(smtp_error_body(&e)))),
        Ok(Ok(Ok(_))) => Ok((StatusCode::OK, Json(json!({"message": EMAIL_SENT_MSG})))),
    }
}

// ============================================================================
// E1e — instance workspaces list/create + slug check.
// ============================================================================

/// `workspace.py:68` (`max_per_page=10, default_per_page=10`): same
/// `ParseError` messages as the shared parser, with the 10-cap.
pub fn parse_instance_per_page(raw: Option<&str>) -> Result<i64, String> {
    let s = raw.unwrap_or("10");
    let v = parse_python_int(s).ok_or_else(|| "Invalid per_page parameter.".to_string())?;
    if v > 10 {
        return Err("Invalid per_page value. Cannot exceed 10.".to_string());
    }
    Ok(v.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64)
}

/// Pure decision bit of `validate_name` (`serializers/workspace.py:23-35`).
pub fn validate_workspace_name(name: &str) -> Result<(), &'static str> {
    if contains_url(name) {
        return Err(NAME_URL_MSG);
    }
    if !has_alphanumeric(name) {
        return Err(NAME_ALNUM_MSG);
    }
    Ok(())
}

/// Pure decision bit of `validate_slug` (`serializers/workspace.py:37-44`)
/// after the DRF format check: restricted → not-valid; taken → in-use.
pub fn validate_workspace_slug(taken: bool, restricted: bool) -> Result<(), &'static str> {
    if restricted {
        return Err(SLUG_NOT_VALID_MSG);
    }
    if taken {
        return Err(SLUG_IN_USE_MSG);
    }
    Ok(())
}

/// DRF `SlugField` format (`[-a-zA-Z0-9_]+`).
pub fn slug_format_valid(slug: &str) -> bool {
    !slug.is_empty()
        && slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// `utils/content_validator.py:246-261` (`str.isalnum` is Unicode-aware;
/// Rust `char::is_alphanumeric` matches).
pub fn has_alphanumeric(value: &str) -> bool {
    value.chars().any(|c| c.is_alphanumeric())
}

/// `utils/url.py:26-54` (length cap, per-line scan, same 4-branch pattern).
pub fn contains_url(value: &str) -> bool {
    if value.len() > 1000 {
        return false;
    }
    let pattern = r"(?i)(?:https?://\S+|www\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*|(?:[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?\.)+[a-zA-Z]{2,6}|(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?))";
    let re = regex::Regex::new(pattern).expect("static url pattern");
    for line in value.split('\n') {
        let hay = if line.len() > 500 { &line[..500] } else { line };
        if re.is_match(hay) {
            return true;
        }
    }
    false
}

/// `utils/constants.py:5-71`.
pub const RESTRICTED_WORKSPACE_SLUGS: &[&str] = &[
    "404",
    "accounts",
    "api",
    "create-workspace",
    "god-mode",
    "installations",
    "invitations",
    "onboarding",
    "profile",
    "spaces",
    "workspace-invitations",
    "password",
    "flags",
    "monitor",
    "monitoring",
    "ingest",
    "plane-pro",
    "plane-ultimate",
    "enterprise",
    "plane-enterprise",
    "disco",
    "silo",
    "chat",
    "calendar",
    "drive",
    "channels",
    "upgrade",
    "billing",
    "sign-in",
    "sign-up",
    "signin",
    "signup",
    "config",
    "live",
    "admin",
    "m",
    "import",
    "importers",
    "integrations",
    "integration",
    "configuration",
    "initiatives",
    "initiative",
    "workflow",
    "workflows",
    "epics",
    "epic",
    "story",
    "mobile",
    "dashboard",
    "desktop",
    "onload",
    "real-time",
    "one",
    "pages",
    "business",
    "pro",
    "settings",
    "license",
    "licenses",
    "instances",
    "instance",
];

#[derive(Debug, Clone, sqlx::FromRow)]
struct InstanceWorkspaceRow {
    id: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    name: String,
    logo: Option<String>,
    slug: String,
    organization_size: Option<String>,
    timezone: String,
    background_color: String,
    logo_asset: Option<String>,
    owner_id: uuid::Uuid,
    owner_email: Option<String>,
    owner_first_name: String,
    owner_last_name: String,
    total_projects: i64,
    total_members: i64,
}

const INSTANCE_WS_SELECT: &str = "w.id, w.created_at, w.updated_at, w.name, w.logo, w.slug, \
    w.organization_size, w.timezone, w.background_color, fa.asset AS logo_asset, \
    w.owner_id, u.email AS owner_email, u.first_name AS owner_first_name, \
    u.last_name AS owner_last_name, \
    (SELECT COUNT(*) FROM projects p WHERE p.workspace_id = w.id AND p.deleted_at IS NULL) AS total_projects, \
    (SELECT COUNT(*) FROM workspace_members wm JOIN users u2 ON u2.id = wm.member_id \
     WHERE wm.workspace_id = w.id AND u2.is_bot = false AND wm.is_active = true \
     AND wm.deleted_at IS NULL) AS total_members";

/// `license/api/serializers/workspace.py:17-21` + `Meta fields="__all__"`
/// (owner nested `UserLiteSerializer`: id/email/first/last).
fn instance_workspace_json(r: &InstanceWorkspaceRow) -> Value {
    let logo_url = r.logo_asset.clone().or_else(|| r.logo.clone());
    json!({
        "id": r.id,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "name": r.name,
        "logo": r.logo,
        "slug": r.slug,
        "organization_size": r.organization_size,
        "timezone": r.timezone,
        "background_color": r.background_color,
        "logo_asset": r.logo_asset,
        "owner": {
            "id": r.owner_id,
            "email": r.owner_email,
            "first_name": r.owner_first_name,
            "last_name": r.owner_last_name,
        },
        "logo_url": logo_url,
        "total_projects": r.total_projects,
        "total_members": r.total_members,
    })
}

#[derive(Debug, Deserialize, Default)]
pub struct InstanceWorkspaceListQuery {
    pub cursor: Option<String>,
    pub per_page: Option<String>,
    pub search: Option<String>,
}

/// GET /api/instances/workspaces/ — `InstanceWorkSpaceEndpoint.get`
/// (`workspace.py:40-69`): `BasePaginator` envelope (reused `DetailEnvelope`
/// — identical key order to `paginator.py:728-743`), per_page default/max
/// 10, `search` over name, `total_projects/total_members` annotations
/// (bots + inactive members excluded, `workspace.py:41-54`).
pub async fn workspaces_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Query(q): Query<InstanceWorkspaceListQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !instance_admin_allowed(&st.pool, auth.0).await? {
        return Ok(deny_detail());
    }
    let per_page = match parse_instance_per_page(q.per_page.as_deref()) {
        Ok(v) => v,
        Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": e})))),
    };
    let page: i128 = match q.cursor.as_deref() {
        None => 0,
        Some(c) => match parse_cursor(c) {
            Ok(cur) => cur.page,
            Err(e) => return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": e})))),
        },
    };
    let search = q.search.unwrap_or_default();
    let like = format!("%{search}%");
    let total: (i64,) = if search.is_empty() {
        sqlx::query_as("SELECT COUNT(*) FROM workspaces WHERE deleted_at IS NULL")
            .fetch_one(&st.pool)
            .await?
    } else {
        sqlx::query_as("SELECT COUNT(*) FROM workspaces WHERE deleted_at IS NULL AND name ILIKE $1")
            .bind(&like)
            .fetch_one(&st.pool)
            .await?
    };
    let total = total.0;
    let limit = per_page.max(0);
    let (rows, next, prev, next_has, prev_has, count, pages): (
        Vec<InstanceWorkspaceRow>,
        String,
        String,
        bool,
        bool,
        i64,
        i64,
    ) = if limit <= 0 {
        (
            vec![],
            next_cursor_str(0, page),
            prev_cursor_str(0, page),
            false,
            page > 0,
            0,
            total_pages(total, 1),
        )
    } else {
        match page_window(page, limit) {
            Err(()) => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"detail": "Invalid cursor parameter."})),
                ));
            }
            Ok(PageWindow::BeyondEnd) => (
                vec![],
                next_cursor_str(limit, page),
                prev_cursor_str(limit, page),
                false,
                true,
                0,
                total_pages(total, limit),
            ),
            Ok(PageWindow::Rows(offset)) => {
                let sql = if search.is_empty() {
                    format!(
                        "SELECT {INSTANCE_WS_SELECT} FROM workspaces w \
                         LEFT JOIN users u ON u.id = w.owner_id \
                         LEFT JOIN file_assets fa ON fa.id = w.logo_asset_id \
                         WHERE w.deleted_at IS NULL ORDER BY w.created_at DESC \
                         LIMIT $1 OFFSET $2"
                    )
                } else {
                    format!(
                        "SELECT {INSTANCE_WS_SELECT} FROM workspaces w \
                         LEFT JOIN users u ON u.id = w.owner_id \
                         LEFT JOIN file_assets fa ON fa.id = w.logo_asset_id \
                         WHERE w.deleted_at IS NULL AND w.name ILIKE $3 \
                         ORDER BY w.created_at DESC LIMIT $1 OFFSET $2"
                    )
                };
                let rows: Vec<InstanceWorkspaceRow> = if search.is_empty() {
                    sqlx::query_as(&sql)
                        .bind(limit)
                        .bind(offset)
                        .fetch_all(&st.pool)
                        .await?
                } else {
                    sqlx::query_as(&sql)
                        .bind(limit)
                        .bind(offset)
                        .bind(&like)
                        .fetch_all(&st.pool)
                        .await?
                };
                let count = rows.len() as i64;
                let has_more = offset + count < total;
                (
                    rows,
                    next_cursor_str(limit, page),
                    prev_cursor_str(limit, page),
                    has_more,
                    page > 0,
                    count,
                    total_pages(total, limit),
                )
            }
        }
    };
    let envelope = DetailEnvelope {
        grouped_by: None,
        sub_grouped_by: None,
        total_count: total,
        next_cursor: next,
        prev_cursor: prev,
        next_page_results: next_has,
        prev_page_results: prev_has,
        count,
        total_pages: pages,
        total_results: total,
        extra_stats: None,
        results: rows.iter().map(instance_workspace_json).collect(),
    };
    Ok((StatusCode::OK, Json(json!(envelope))))
}

/// POST /api/instances/workspaces/ — `InstanceWorkSpaceEndpoint.post`
/// (`workspace.py:71-110`): **201**; name+slug required, 80/48 lengths,
/// serializer name/slug validators (400 array shape `:100-103`); race dup
/// → **409** `{"slug": …}` (`:105-110`); owner + `WorkspaceMember(role=20)`
/// in one tx (Celery skipped).
pub async fn workspaces_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !instance_admin_allowed(&st.pool, auth.0).await? {
        return Ok(deny_detail());
    }
    let name = body
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let slug = body
        .get("slug")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    // `workspace.py:78-82`.
    if name.trim().is_empty() || slug.trim().is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": NAME_SLUG_REQUIRED_MSG})),
        ));
    }
    // `workspace.py:84-88` (Python `len` = chars).
    if name.chars().count() > 80 || slug.chars().count() > 48 {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": NAME_SLUG_LENGTH_MSG})),
        ));
    }
    // DRF `SlugField` format runs before model validators.
    if !slug_format_valid(&slug) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!([SLUG_FORMAT_MSG]))));
    }
    if let Err(e) = validate_workspace_name(&name) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!([e]))));
    }
    let taken: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM workspaces WHERE slug ILIKE $1 AND deleted_at IS NULL)",
    )
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    // `workspace.py:31` combines iexact-exists OR restricted for the
    // availability check; the serializer pre-check is iexact-only, with the
    // restricted list enforced by `slug_validator` (`db/models/workspace.py`).
    if let Err(e) =
        validate_workspace_slug(taken, RESTRICTED_WORKSPACE_SLUGS.contains(&slug.as_str()))
    {
        return Ok((StatusCode::BAD_REQUEST, Json(json!([e]))));
    }
    let bg = format!("#{:06X}", rand::random::<u32>() & 0xFFFFFF);
    let company_role = body
        .get("company_role")
        .and_then(Value::as_str)
        .unwrap_or("");
    let mut tx = st.pool.begin().await?;
    // `workspace.py:91` (`serializer.save(owner=request.user)`).
    let ws: Result<Option<(uuid::Uuid,)>, sqlx::Error> = sqlx::query_as(
        "INSERT INTO workspaces (id, name, slug, owner_id, timezone, background_color, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, 'UTC', $4, now(), now()) RETURNING id",
    )
    .bind(&name)
    .bind(&slug)
    .bind(auth.0)
    .bind(&bg)
    .fetch_optional(&mut *tx)
    .await;
    let ws_id = match ws {
        Ok(v) => v,
        // `workspace.py:105-110` (race dup → 409, `slug` key).
        Err(e) if is_constraint_violation(&e) => {
            tx.rollback().await?;
            return Ok((StatusCode::CONFLICT, Json(json!({"slug": SLUG_EXISTS_MSG}))));
        }
        Err(e) => return Err(e.into()),
    };
    let Some((ws_id,)) = ws_id else {
        tx.rollback().await?;
        return Ok(missing());
    };
    // `workspace.py:93-98` (member row, role 20).
    let member = sqlx::query(
        "INSERT INTO workspace_members (id, workspace_id, member_id, role, company_role, \
         view_props, default_props, issue_props, is_active, getting_started_checklist, tips, \
         explored_features, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, 20, $3, '{}', '{}', \
         '{\"subscribed\": true, \"assigned\": true, \"created\": true, \"all_issues\": true}', \
         true, '{}', '{}', '{}', now(), now())",
    )
    .bind(ws_id)
    .bind(auth.0)
    .bind(company_role)
    .execute(&mut *tx)
    .await;
    if let Err(e) = member {
        tx.rollback().await?;
        if is_constraint_violation(&e) {
            return Ok((StatusCode::CONFLICT, Json(json!({"slug": SLUG_EXISTS_MSG}))));
        }
        return Err(e.into());
    }
    tx.commit().await?;
    let sql = format!(
        "SELECT {INSTANCE_WS_SELECT} FROM workspaces w \
         LEFT JOIN users u ON u.id = w.owner_id \
         LEFT JOIN file_assets fa ON fa.id = w.logo_asset_id \
         WHERE w.id = $1"
    );
    match sqlx::query_as::<_, InstanceWorkspaceRow>(&sql)
        .bind(ws_id)
        .fetch_optional(&st.pool)
        .await?
    {
        Some(row) => Ok((StatusCode::CREATED, Json(instance_workspace_json(&row)))),
        None => Ok(missing()),
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct SlugCheckQuery {
    pub slug: Option<String>,
}

/// GET /api/instances/workspace-slug-check/ —
/// `InstanceWorkSpaceAvailabilityCheckEndpoint.get` (`workspace.py:19-32`):
/// missing → 400 verbatim; else 200 `{"status"}` = NOT(iexact-exists OR
/// restricted).
pub async fn slug_check(
    State(st): State<AppState>,
    auth: AuthUser,
    Query(q): Query<SlugCheckQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !instance_admin_allowed(&st.pool, auth.0).await? {
        return Ok(deny_detail());
    }
    let slug = q.slug.unwrap_or_default();
    // `workspace.py:25-29`.
    if slug.trim().is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": SLUG_CHECK_REQUIRED_MSG})),
        ));
    }
    // `workspace.py:31` (iexact match; restricted list is case-sensitive).
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM workspaces WHERE slug ILIKE $1 AND deleted_at IS NULL)",
    )
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    let unavailable = exists || RESTRICTED_WORKSPACE_SLUGS.contains(&slug.as_str());
    Ok((StatusCode::OK, Json(json!({"status": !unavailable}))))
}

// ============================================================================
// Unit tests (pure fns — no DB/Redis).
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_code_consts_match_django() {
        // `authentication/adapter/error.py:62-71`.
        assert_eq!(INSTANCE_NOT_CONFIGURED_CODE, 5000);
        assert_eq!(ADMIN_ALREADY_EXIST_CODE, 5150);
        assert_eq!(REQUIRED_ADMIN_EMAIL_PASSWORD_FIRST_NAME_CODE, 5155);
        assert_eq!(INVALID_ADMIN_EMAIL_CODE, 5160);
        assert_eq!(REQUIRED_ADMIN_EMAIL_PASSWORD_CODE, 5170);
        assert_eq!(ADMIN_AUTHENTICATION_FAILED_CODE, 5175);
        assert_eq!(ADMIN_USER_ALREADY_EXIST_CODE, 5180);
        assert_eq!(ADMIN_USER_DOES_NOT_EXIST_CODE, 5185);
        assert_eq!(ADMIN_USER_DEACTIVATED_CODE, 5190);
        assert_eq!(PASSWORD_TOO_WEAK_CODE, 5021);
    }

    #[test]
    fn error_status_mapping() {
        assert_eq!(admin_error_status(5175), StatusCode::UNAUTHORIZED);
        assert_eq!(admin_error_status(5190), StatusCode::FORBIDDEN);
        for code in [5000, 5150, 5155, 5160, 5170, 5180, 5185, 5021] {
            assert_eq!(admin_error_status(code), StatusCode::BAD_REQUEST, "{code}");
        }
    }

    #[test]
    fn gate_role_threshold() {
        // `license/api/permissions/instance.py:18` (`role__gte=15`).
        assert!(gate_role_allowed(Some(20)));
        assert!(gate_role_allowed(Some(15)));
        assert!(!gate_role_allowed(Some(5)));
        assert!(!gate_role_allowed(None));
        assert!(!gate_role_allowed(Some(14)));
    }

    #[test]
    fn client_ip_precedence() {
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 8000);
        let mut h = HeaderMap::new();
        // No XFF → peer.
        assert_eq!(client_ip_from(&h, Some(peer)), "10.0.0.1");
        // XFF first entry wins.
        h.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8".parse().unwrap());
        assert_eq!(client_ip_from(&h, Some(peer)), "1.2.3.4");
        // Neither → 0.0.0.0.
        assert_eq!(client_ip_from(&HeaderMap::new(), None), "0.0.0.0");
    }

    #[test]
    fn telemetry_parsing() {
        assert!(parse_telemetry(None));
        assert!(parse_telemetry(Some(&json!(true))));
        assert!(!parse_telemetry(Some(&json!(false))));
        assert!(parse_telemetry(Some(&json!("True"))));
        assert!(!parse_telemetry(Some(&json!("False"))));
        assert!(!parse_telemetry(Some(&json!("0"))));
        assert!(parse_telemetry(Some(&json!("1"))));
    }

    #[test]
    fn fernet_roundtrip() {
        let secret = "unit-test-secret";
        let cases = [
            "EMAIL_HOST_PASSWORD_VALUE".to_string(),
            "x".to_string(),
            "unicode-ü-✓-long-".repeat(50),
        ];
        for data in &cases {
            let enc = encrypt_data(data, secret);
            assert!(!enc.is_empty());
            assert_ne!(enc, *data);
            assert_eq!(decrypt_data(&enc, secret), *data);
        }
        assert_eq!(encrypt_data("", secret), "");
        assert_eq!(decrypt_data("", secret), "");
    }

    #[test]
    fn fernet_decrypts_django_vector() {
        // Produced by Django's exact `derive_key`+`Fernet.encrypt`
        // (`license/utils/encryption.py`), SECRET_KEY=test-secret-key-for-vector.
        let secret = "test-secret-key-for-vector";
        let token = "gAAAAABqnOt8Rib5noA6dLDu1XRvKTzNwAo_ZVDkTIJRPQyyhB3RpBuLJIYjiFiXCR-49dbwAuWrnXBE7nodmHkfxMrR76TehfM_hYgvdV_iS1Sc0nQNPeI=";
        assert_eq!(decrypt_data(token, secret), "EMAIL_HOST_PASSWORD_VALUE");
        // Wrong key → "" (Django logs + returns "").
        assert_eq!(decrypt_data(token, "wrong-key"), "");
        // Tampered token → "".
        let mut bad = token.to_string();
        bad.replace_range(10..12, "AA");
        assert_eq!(decrypt_data(&bad, secret), "");
    }

    #[test]
    fn per_page_10_defaults_and_cap() {
        // `workspace.py:68` (default 10, max 10).
        assert_eq!(parse_instance_per_page(None).unwrap(), 10);
        assert_eq!(parse_instance_per_page(Some("5")).unwrap(), 5);
        assert_eq!(
            parse_instance_per_page(Some("11")).unwrap_err(),
            "Invalid per_page value. Cannot exceed 10."
        );
        assert_eq!(
            parse_instance_per_page(Some("abc")).unwrap_err(),
            "Invalid per_page parameter."
        );
    }

    #[test]
    fn workspace_name_validation() {
        assert!(validate_workspace_name("Acme Corp").is_ok());
        assert_eq!(
            validate_workspace_name("see https://example.com now").unwrap_err(),
            NAME_URL_MSG
        );
        assert_eq!(
            validate_workspace_name("-_________-").unwrap_err(),
            NAME_ALNUM_MSG
        );
        // Unicode letters count (`content_validator.py:261` isalnum).
        assert!(validate_workspace_name("日本語ワークスペース").is_ok());
    }

    #[test]
    fn workspace_slug_validation() {
        assert!(validate_workspace_slug(false, false).is_ok());
        assert_eq!(
            validate_workspace_slug(false, true).unwrap_err(),
            SLUG_NOT_VALID_MSG
        );
        assert_eq!(
            validate_workspace_slug(true, false).unwrap_err(),
            SLUG_IN_USE_MSG
        );
        assert!(slug_format_valid("acme-1_x"));
        assert!(!slug_format_valid("has space"));
        assert!(!slug_format_valid("has/slash"));
        assert!(!slug_format_valid(""));
    }

    #[test]
    fn restricted_slugs_shape() {
        // Spot-check `utils/constants.py:5-71` parity (incl. the `"config"`
        // duplicate Django carries at :38 and :49 — preserved here).
        for s in ["admin", "api", "god-mode", "instances", "sign-in"] {
            assert!(RESTRICTED_WORKSPACE_SLUGS.contains(&s), "{s}");
        }
        assert!(!RESTRICTED_WORKSPACE_SLUGS.contains(&" acme "));
    }

    #[test]
    fn instance_patch_allowlist() {
        // Read-only keys (`serializers/instance.py:17`) are not writable.
        for k in ["id", "email", "last_checked_at", "is_setup_done", "nope"] {
            assert!(parse_instance_patch_value(k, &json!("x")).is_none(), "{k}");
        }
        assert_eq!(
            parse_instance_patch_value("instance_name", &json!("Acme")),
            Some(InstancePatchVal::Text(Some("Acme".to_string())))
        );
        assert_eq!(
            parse_instance_patch_value("is_telemetry_enabled", &json!(true)),
            Some(InstancePatchVal::Flag(true))
        );
        assert_eq!(
            parse_instance_patch_value("is_verified", &json!("0")),
            Some(InstancePatchVal::Flag(false))
        );
        assert_eq!(
            parse_instance_patch_value("domain", &json!(null)),
            Some(InstancePatchVal::Text(None))
        );
        assert!(parse_instance_patch_value("is_test", &json!("maybe")).is_none());
    }

    #[test]
    fn smtp_mapping_shapes() {
        // 535 permanent → SMTPAuthenticationError body verbatim.
        assert_eq!(
            smtp_body_for_permanent(true, true, "535 Authentication failed"),
            json!({"error": "Invalid credentials provided"})
        );
        // 5xx naming the sender → SMTPSenderRefused body.
        assert_eq!(
            smtp_body_for_permanent(true, false, "550 Sender address rejected"),
            json!({"error": "From address is invalid."})
        );
        // Other 5xx → SMTPRecipientsRefused body.
        assert_eq!(
            smtp_body_for_permanent(true, false, "550 Recipient address rejected"),
            json!({"error": "All recipient addresses were refused."})
        );
        // io kinds → TimeoutError / ConnectionError bodies verbatim.
        use std::io::ErrorKind;
        assert_eq!(
            smtp_body_for_io(ErrorKind::TimedOut),
            Some(json!({"error": "Timeout error while trying to connect to the SMTP server."}))
        );
        assert_eq!(
            smtp_body_for_io(ErrorKind::NetworkUnreachable),
            Some(
                json!({"error": "Network connection error. Please check your internet connection."})
            )
        );
        assert_eq!(smtp_body_for_io(ErrorKind::ConnectionRefused), None);
    }

    #[test]
    fn conflict_and_detail_shapes() {
        // `workspace.py:107-109` (409 keeps the `slug` key).
        let body = json!({"slug": SLUG_EXISTS_MSG});
        assert_eq!(
            body,
            json!({"slug": "The workspace with the slug already exists"})
        );
        // Deny is the DRF permission-class shape, not the app `deny()`.
        let (_, Json(deny)) = deny_detail();
        assert_eq!(
            deny,
            json!({"detail": "You do not have permission to perform this action."})
        );
    }
}
