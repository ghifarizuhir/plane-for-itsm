// crates/api/src/routes/auth_signup.rs
//! POST /api/auth/signup/ — registrasi member invite-only (Rust).
//!
//! Paritas semantik Django `SignUpAuthEndpoint.post`
//! (`apps/api/plane/authentication/views/app/email.py:140-244`) +
//! `EmailProvider(is_signup=True)`. Perbedaan yang disengaja (desain
//! `docs/superpowers/specs/2026-09-20-rust-auth-signup-design.md`):
//! - **Invite-only**: hanya email dengan `workspace_member_invites` pending
//!   (`deleted_at IS NULL AND responded_at IS NULL`) yang boleh signup →
//!   403 `SIGNUP_DISABLED` (5015). Django default open (`ENABLE_SIGNUP=1`);
//!   gate ini menggantikannya.
//! - **Auto-join**: invite pending langsung diproses jadi `WorkspaceMember`
//!   (reactivate/insert) dan row invite di-hard-delete dalam transaksi yang
//!   sama, plus `profiles.last_workspace_id` di-set.
//! - Email/celery tidak dikirim (SMTP tidak dikonfigurasi; parity invite api-rs).
//! - `ENABLE_EMAIL_PASSWORD` tidak dicek (parity `routes::auth::login`).
//!
//! Kode error disalin dari `authentication/adapter/error.py:7-26`; bentuk body
//! `{error_code, error_message}` (`AuthenticationException.get_error_dict`).

use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::routes::auth::{email_valid, family_key, set_cookies};
use crate::state::AppState;
use common::auth as authn;

const ACCESS_TTL_SECS: i64 = 900;
const REFRESH_TTL_SECS: i64 = 30 * 24 * 3600;

// Kode error Django (`adapter/error.py`).
pub const INSTANCE_NOT_CONFIGURED: i32 = 5000;
pub const SIGNUP_DISABLED: i32 = 5015;
pub const PASSWORD_TOO_WEAK: i32 = 5021;
pub const USER_ALREADY_EXIST: i32 = 5030;
pub const REQUIRED_EMAIL_PASSWORD_SIGN_UP: i32 = 5040;
pub const INVALID_EMAIL_SIGN_UP: i32 = 5045;

#[derive(Deserialize)]
pub struct SignupBody {
    pub email: Option<String>,
    pub password: Option<String>,
}

/// Mapping kode error → HTTP status (kontrak desain).
/// `SIGNUP_DISABLED` invite-only → 403; `USER_ALREADY_EXIST` → 409; sisanya 400
/// (termasuk `INSTANCE_NOT_CONFIGURED`, selaras `email_check`).
pub fn signup_error_status(code: i32) -> StatusCode {
    match code {
        SIGNUP_DISABLED => StatusCode::FORBIDDEN,
        USER_ALREADY_EXIST => StatusCode::CONFLICT,
        _ => StatusCode::BAD_REQUEST,
    }
}

fn auth_error(code: i32, message: &str) -> Json<Value> {
    Json(json!({"error_code": code, "error_message": message}))
}

fn err(code: i32, message: &str) -> (StatusCode, HeaderMap, Json<Value>) {
    (
        signup_error_status(code),
        HeaderMap::new(),
        auth_error(code, message),
    )
}

/// DB error → 500 generik + log (pola `auth::login`: teks driver tidak boleh
/// bocor ke body response pada surface auth publik).
fn db_err(e: sqlx::Error) -> common::errors::AppError {
    tracing::warn!(error=%e, "auth signup: db error");
    common::errors::AppError::internal()
}

/// `if not email or not password` Django pada nilai RAW (sebelum strip):
/// string berisi spasi dianggap terisi → lolos ke validasi email berikutnya.
pub fn signup_fields_missing(email_raw: &str, password_raw: &str) -> bool {
    email_raw.is_empty() || password_raw.is_empty()
}

/// POST /api/auth/signup/ — JSON `{email, password}`.
///
/// Urutan: instance setup (5000) → field wajib (5040) → email valid (5045) →
/// sudah terdaftar (5030) → gate invite-only (5015) → password kuat (5021).
/// Sukses: user + profile + auto-join + hapus invite (satu tx), lalu cookie
/// sesi seperti login → 200 `{id, email}`.
pub async fn signup(
    State(st): State<AppState>,
    addr: Option<ConnectInfo<std::net::SocketAddr>>,
    headers: HeaderMap,
    Json(body): Json<SignupBody>,
) -> Result<(StatusCode, HeaderMap, Json<Value>), common::errors::AppError> {
    // 1. Instance setup — pola `auth::email_check` (`auth.rs:119-126`).
    let setup: Option<bool> = sqlx::query_scalar(
        "SELECT is_setup_done FROM instances WHERE deleted_at IS NULL ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_optional(&st.pool)
    .await.map_err(db_err)?;
    if setup != Some(true) {
        return Ok(err(INSTANCE_NOT_CONFIGURED, "INSTANCE_NOT_CONFIGURED"));
    }
    // 2. Wajib diisi — nilai RAW (`email.py:163`).
    let email_raw = body.email.unwrap_or_default();
    let password = body.password.unwrap_or_default();
    if signup_fields_missing(&email_raw, &password) {
        return Ok(err(
            REQUIRED_EMAIL_PASSWORD_SIGN_UP,
            "REQUIRED_EMAIL_PASSWORD_SIGN_UP",
        ));
    }
    // 3. Normalisasi + validasi (`email.py:178-194`).
    let email = email_raw.trim().to_lowercase();
    if !email_valid(&email) {
        return Ok(err(INVALID_EMAIL_SIGN_UP, "INVALID_EMAIL_SIGN_UP"));
    }
    // 4. Sudah terdaftar (`email.py:197-212`).
    let taken: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)")
        .bind(&email)
        .fetch_one(&st.pool)
        .await.map_err(db_err)?;
    if taken {
        return Ok(err(USER_ALREADY_EXIST, "USER_ALREADY_EXIST"));
    }
    // 5. Gate invite-only (keputusan desain, menggantikan `__check_signup`).
    let invited: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM workspace_member_invites \
         WHERE email = $1 AND deleted_at IS NULL AND responded_at IS NULL)",
    )
    .bind(&email)
    .fetch_one(&st.pool)
    .await.map_err(db_err)?;
    if !invited {
        return Ok(err(SIGNUP_DISABLED, "SIGNUP_DISABLED"));
    }
    // 6. Kekuatan password (`adapter/base.py:90-100`, zxcvbn >= 3).
    if !crate::routes::auth_compat::password_strong_enough(&password) {
        return Ok(err(PASSWORD_TOO_WEAK, "PASSWORD_TOO_WEAK"));
    }

    let ip = crate::routes::instance_admin::client_ip_from(&headers, addr.map(|a| a.0));
    let ua = crate::routes::instance_admin::user_agent_of(&headers);
    let username = uuid::Uuid::new_v4().simple().to_string();
    let hash = authn::make_django_password(&password);
    let display_name = email.split('@').next().unwrap_or(&email).to_string();
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );

    let mut tx = st.pool.begin().await.map_err(db_err)?;

    // Invariant gate di dalam tx: invite harus masih ada saat tx dimulai
    // (mempersempit, bukan menutup, jendela balapan admin menghapus invite
    // antara pre-check dan tx; parity Django).
    let invites: Vec<(uuid::Uuid, uuid::Uuid, i16)> = sqlx::query_as(
        "SELECT id, workspace_id, role FROM workspace_member_invites \
         WHERE email = $1 AND deleted_at IS NULL AND responded_at IS NULL \
         ORDER BY created_at ASC",
    )
    .bind(&email)
    .fetch_all(&mut *tx)
    .await.map_err(db_err)?;
    if invites.is_empty() {
        tx.rollback().await.map_err(db_err)?;
        return Ok(err(SIGNUP_DISABLED, "SIGNUP_DISABLED"));
    }

    // users — kolom/nilai persis `instance_admin::sign_up` (`:594-616`),
    // `is_email_verified=false` (default password signup Django).
    // `ON CONFLICT (email)`: balapan signup email sama → loser 409, bukan 500
    // unique-violation dengan teks driver.
    let inserted: Option<(uuid::Uuid,)> = sqlx::query_as(
        "INSERT INTO users (id, email, username, password, first_name, last_name, display_name, \
         avatar, date_joined, token, user_timezone, last_location, created_location, \
         last_login_ip, last_logout_ip, last_login_medium, last_login_uagent, last_active, \
         last_login_time, token_updated_at, is_active, is_staff, is_superuser, is_managed, \
         is_password_expired, is_email_verified, is_password_autoset, is_bot, is_email_valid, \
         is_password_reset_required, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, '', '', $4, '', now(), $5, 'UTC', '', '', \
         $6, '', 'email', $7, now(), now(), now(), true, false, false, false, false, false, \
         false, false, true, false, now(), now()) ON CONFLICT (email) DO NOTHING RETURNING id",
    )
    .bind(&email)
    .bind(&username)
    .bind(&hash)
    .bind(&display_name)
    .bind(&token)
    .bind(&ip)
    .bind(&ua)
    .fetch_optional(&mut *tx)
    .await.map_err(db_err)?;
    let Some((uid,)) = inserted else {
        tx.rollback().await.map_err(db_err)?;
        return Ok(err(USER_ALREADY_EXIST, "USER_ALREADY_EXIST"));
    };

    // profiles — kolom/nilai persis `instance_admin::sign_up` (`:617-637`)
    // dengan `company_name` '' (default Django member).
    let bg = format!("#{:06X}", rand::random::<u32>() & 0xFFFFFF);
    sqlx::query(
        "INSERT INTO profiles (id, user_id, theme, is_tour_completed, onboarding_step, \
         is_onboarded, billing_address_country, has_billing_address, company_name, \
         is_mobile_onboarded, mobile_onboarding_step, mobile_timezone_auto_set, language, \
         is_smooth_cursor_enabled, start_of_the_week, is_app_rail_docked, background_color, \
         goals, has_marketing_email_consent, is_navigation_tour_completed, \
         is_subscribed_to_changelog, notification_view_mode, product_tour, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, '{}', false, $2, false, 'INDIA', false, '', false, $3, \
         false, 'en', false, 0, true, $4, '{}', false, false, false, 'full', $5, now(), now())",
    )
    .bind(uid)
    .bind(json!({"profile_complete": false, "workspace_create": false, "workspace_invite": false, "workspace_join": false}))
    .bind(json!({"profile_complete": false, "workspace_create": false, "workspace_join": false}))
    .bind(&bg)
    .bind(json!({"work_items": false, "cycles": false, "modules": false, "intake": false, "pages": false}))
    .execute(&mut *tx)
    .await.map_err(db_err)?;

    // Auto-join: semua invite pending → WorkspaceMember (reactivate/insert),
    // row invite di-hard-delete; `last_workspace_id` = workspace pertama.
    let mut first_workspace: Option<uuid::Uuid> = None;
    for (invite_id, workspace_id, role) in &invites {
        let existing: Option<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT id FROM workspace_members WHERE workspace_id = $1 AND member_id = $2",
        )
        .bind(workspace_id)
        .bind(uid)
        .fetch_optional(&mut *tx)
        .await.map_err(db_err)?;
        if existing.is_some() {
            sqlx::query(
                "UPDATE workspace_members SET is_active = true, role = $1, updated_at = now() \
                 WHERE workspace_id = $2 AND member_id = $3",
            )
            .bind(role)
            .bind(workspace_id)
            .bind(uid)
            .execute(&mut *tx)
            .await.map_err(db_err)?;
        } else {
            let (view_props, default_props, issue_props) =
                crate::routes::invite::default_ws_member_props();
            sqlx::query(
                "INSERT INTO workspace_members (id, workspace_id, member_id, role, view_props, \
                 default_props, issue_props, is_active, getting_started_checklist, tips, \
                 explored_features, created_at, updated_at) \
                 VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, true, '{}', '{}', '{}', now(), now()) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(workspace_id)
            .bind(uid)
            .bind(role)
            .bind(&view_props)
            .bind(&default_props)
            .bind(&issue_props)
            .execute(&mut *tx)
            .await.map_err(db_err)?;
        }
        sqlx::query("DELETE FROM workspace_member_invites WHERE id = $1")
            .bind(invite_id)
            .execute(&mut *tx)
            .await.map_err(db_err)?;
        if first_workspace.is_none() {
            first_workspace = Some(*workspace_id);
        }
    }
    if let Some(ws) = first_workspace {
        sqlx::query(
            "UPDATE profiles SET last_workspace_id = $1, updated_at = now() WHERE user_id = $2",
        )
        .bind(ws)
        .bind(uid)
        .execute(&mut *tx)
        .await.map_err(db_err)?;
    }
    tx.commit().await.map_err(db_err)?;

    // Sesi cookie — persis `routes::auth::login` (`auth.rs:204-247`).
    let access = authn::encode_access(&uid, &st.config.jwt_secret, ACCESS_TTL_SECS);
    let (hash_rt, raw_rt) = authn::new_refresh();
    let family = uuid::Uuid::new_v4().to_string();
    let mut conn = st.redis_client().await.map_err(|e| {
        tracing::warn!(error=%e, "auth signup: redis unavailable");
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
            tracing::warn!(error=%e, "auth signup: refresh store failed");
            common::errors::AppError::internal()
        })?;
    redis::cmd("SADD")
        .arg(family_key(&family))
        .arg(&hash_rt)
        .query_async::<()>(&mut conn)
        .await
        .map_err(|e| {
            tracing::warn!(error=%e, "auth signup: family store failed");
            common::errors::AppError::internal()
        })?;
    redis::cmd("EXPIRE")
        .arg(family_key(&family))
        .arg(REFRESH_TTL_SECS)
        .query_async::<()>(&mut conn)
        .await
        .map_err(|e| {
            tracing::warn!(error=%e, "auth signup: family expire failed");
            common::errors::AppError::internal()
        })?;
    let mut out = HeaderMap::new();
    set_cookies(&mut out, &access, &raw_rt, st.config.cookie_secure);
    Ok((
        StatusCode::OK,
        out,
        Json(json!({"id": uid, "email": email})),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_status_mapping() {
        assert_eq!(signup_error_status(SIGNUP_DISABLED), StatusCode::FORBIDDEN);
        assert_eq!(signup_error_status(USER_ALREADY_EXIST), StatusCode::CONFLICT);
        assert_eq!(signup_error_status(PASSWORD_TOO_WEAK), StatusCode::BAD_REQUEST);
        assert_eq!(signup_error_status(INVALID_EMAIL_SIGN_UP), StatusCode::BAD_REQUEST);
        assert_eq!(
            signup_error_status(REQUIRED_EMAIL_PASSWORD_SIGN_UP),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(signup_error_status(INSTANCE_NOT_CONFIGURED), StatusCode::BAD_REQUEST);
        assert_eq!(signup_error_status(9999), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn raw_required_semantics() {
        assert!(signup_fields_missing("", "whatever1!"));
        assert!(signup_fields_missing("a@b.co", ""));
        assert!(signup_fields_missing("", ""));
        // Spasi dianggap terisi (nilai raw), sama seperti Python truthiness.
        assert!(!signup_fields_missing("   ", "whatever1!"));
        assert!(!signup_fields_missing("a@b.co", "   "));
    }

    #[test]
    fn error_body_shape() {
        let body = auth_error(SIGNUP_DISABLED, "SIGNUP_DISABLED");
        assert_eq!(body.0["error_code"], 5015);
        assert_eq!(body.0["error_message"], "SIGNUP_DISABLED");
    }

    #[test]
    fn family_key_shape() {
        assert_eq!(family_key("abc"), "auth:family:abc");
    }
}
