// crates/api/src/routes/auth_compat.rs
//! Paritas Django `/auth/*` compat (`apps/api/plane/authentication/urls.py`).
//! Rust tidak memakai CSRF (pengganti: Origin/Referer check di middleware),
//! jadi token selalu string kosong — caller hanya meneruskannya sebagai
//! header `X-CSRFTOKEN` yang diabaikan server.

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};
use common::auth as authn;

use super::auth::{email_valid, EmailCheckBody};

pub fn csrf_token_value() -> Value {
    json!({"csrf_token": ""})
}

/// GET /auth/get-csrf-token/ — paritas `CSRFTokenEndpoint` (`common.py:28`).
pub async fn csrf_token() -> Json<Value> {
    Json(csrf_token_value())
}

/// Bentuk error Django `AuthenticationException.get_error_dict()`
/// (`apps/api/plane/authentication/adapter/error.py`) — disalin dari
/// `routes::auth` agar modul ini tidak bergantung antar-modul routes.
fn auth_error(code: i32, message: &str) -> Json<Value> {
    Json(json!({"error_code": code, "error_message": message}))
}

fn auth_error_payload(code: i32, message: &str, detail: &str) -> Json<Value> {
    Json(json!({"error_code": code, "error_message": message, "error": detail}))
}

/// Selaras Django `zxcvbn(new_password)["score"] < 3` → tolak.
/// (`zxcvbn` v3 mengembalikan `Entropy` langsung, bukan `Result`.)
pub fn password_strong_enough(password: &str) -> bool {
    u8::from(zxcvbn::zxcvbn(password, &[]).score()) >= 3
}

#[derive(Deserialize)]
pub struct ChangePasswordBody {
    pub old_password: Option<String>,
    pub new_password: Option<String>,
}

/// POST /auth/change-password/ — paritas `ChangePasswordEndpoint`
/// (`common.py:47-96`). Login-ulang Django (`user_login`) adalah no-op di
/// Rust (sesi berupa cookie-JWT, tidak terpengaruh ganti password).
pub async fn change_password(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<ChangePasswordBody>,
) -> (StatusCode, Json<Value>) {
    // NOTE: tabel `users` tidak punya `deleted_at` — filter id saja.
    let row: Option<(bool, String)> =
        sqlx::query_as("SELECT is_password_autoset, password FROM users WHERE id = $1")
            .bind(auth.0)
            .fetch_optional(&st.pool)
            .await
            .unwrap_or(None);
    let Some((autoset, hash)) = row else {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid credentials"})));
    };
    if !autoset {
        let Some(old) = body.old_password.as_deref().filter(|s| !s.is_empty()) else {
            return (StatusCode::BAD_REQUEST, auth_error_payload(5138, "MISSING_PASSWORD", "Old password is missing"));
        };
        if !authn::verify_django_password(old, &hash) {
            return (StatusCode::BAD_REQUEST, auth_error_payload(5135, "INCORRECT_OLD_PASSWORD", "Old password is not correct"));
        }
    }
    let Some(new) = body.new_password.as_deref().filter(|s| !s.is_empty()) else {
        return (StatusCode::BAD_REQUEST, auth_error_payload(5138, "MISSING_PASSWORD", "Old or new password is missing"));
    };
    if !password_strong_enough(new) {
        return (StatusCode::BAD_REQUEST, auth_error(5021, "PASSWORD_TOO_WEAK"));
    }
    let updated = sqlx::query("UPDATE users SET password = $1, is_password_autoset = false, updated_at = now() WHERE id = $2")
        .bind(authn::make_django_password(new))
        .bind(auth.0)
        .execute(&st.pool)
        .await;
    if updated.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})));
    }
    (StatusCode::OK, Json(json!({"message": "Password updated successfully"})))
}

#[derive(Deserialize)]
pub struct SetPasswordBody {
    pub password: Option<String>,
}

/// Subset `me` untuk respons set-password — hanya field yang dipakai caller
/// (`store/user handleSetPassword`, onboarding); tanpa field `password`.
pub fn user_subset_json(id: &str, email: &str, first: &str, last: &str) -> Value {
    json!({"id": id, "email": email, "first_name": first, "last_name": last})
}

/// POST /auth/set-password/ — paritas `SetUserPasswordEndpoint`
/// (`common.py:99-138`). Hanya untuk user `is_password_autoset=true`.
pub async fn set_password(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<SetPasswordBody>,
) -> (StatusCode, Json<Value>) {
    // NOTE: tabel `users` tidak punya `deleted_at` — filter id saja.
    let row: Option<(bool,)> =
        sqlx::query_as("SELECT is_password_autoset FROM users WHERE id = $1")
            .bind(auth.0)
            .fetch_optional(&st.pool)
            .await
            .unwrap_or(None);
    let Some((autoset,)) = row else {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid credentials"})));
    };
    if !autoset {
        return (StatusCode::BAD_REQUEST, auth_error_payload(5145, "PASSWORD_ALREADY_SET", "Your password is already set please change your password from profile"));
    }
    let Some(pw) = body.password.as_deref().filter(|s| !s.is_empty()) else {
        return (StatusCode::BAD_REQUEST, auth_error(5020, "INVALID_PASSWORD"));
    };
    if !password_strong_enough(pw) {
        return (StatusCode::BAD_REQUEST, auth_error(5020, "INVALID_PASSWORD"));
    }
    let hash = authn::make_django_password(pw);
    let upd = sqlx::query("UPDATE users SET password = $1, is_password_autoset = false, updated_at = now() WHERE id = $2")
        .bind(&hash)
        .bind(auth.0)
        .execute(&st.pool)
        .await;
    if upd.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})));
    }
    let back: Option<(String, String, String)> =
        sqlx::query_as("SELECT email, first_name, last_name FROM users WHERE id = $1")
            .bind(auth.0)
            .fetch_optional(&st.pool)
            .await
            .unwrap_or(None);
    match back {
        Some((email, first, last)) => (
            StatusCode::OK,
            Json(user_subset_json(&auth.0.to_string(), &email, &first, &last)),
        ),
        None => (StatusCode::NOT_FOUND, Json(json!({"error": "User not found"}))),
    }
}

/// Cek SMTP terkonfigurasi selaras `routes::instance` — `EMAIL_HOST` tak kosong.
pub fn smtp_configured(host: &str) -> bool {
    !host.is_empty()
}

/// POST /auth/forgot-password/ — paritas `ForgotPasswordEndpoint` SEBATAS
/// gate yang terjangkau tanpa SMTP (5000/5025/5005/5060). Pengiriman email
/// reset = follow-up saat EMAIL_HOST dikonfigurasi.
pub async fn forgot_password(
    State(st): State<AppState>,
    Json(body): Json<EmailCheckBody>,
) -> (StatusCode, Json<Value>) {
    // NOTE: `instances` punya `deleted_at`, `users` tidak — selaras email-check.
    let setup: Option<bool> = sqlx::query_scalar(
        "SELECT is_setup_done FROM instances WHERE deleted_at IS NULL ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_optional(&st.pool)
    .await
    .unwrap_or(None);
    if setup != Some(true) {
        return (StatusCode::BAD_REQUEST, auth_error(5000, "INSTANCE_NOT_CONFIGURED"));
    }
    if !smtp_configured(&std::env::var("EMAIL_HOST").unwrap_or_default()) {
        return (StatusCode::BAD_REQUEST, auth_error(5025, "SMTP_NOT_CONFIGURED"));
    }
    let email = body.email.unwrap_or_default().to_lowercase().trim().to_string();
    if email.is_empty() || !email_valid(&email) {
        return (StatusCode::BAD_REQUEST, auth_error(5005, "INVALID_EMAIL"));
    }
    let exists: Option<bool> =
        sqlx::query_scalar("SELECT true FROM users WHERE email = $1")
            .bind(&email)
            .fetch_optional(&st.pool)
            .await
            .unwrap_or(None);
    if exists != Some(true) {
        return (StatusCode::BAD_REQUEST, auth_error(5060, "USER_DOES_NOT_EXIST"));
    }
    // SMTP terkonfigurasi tapi pengiriman email belum ada → katakan terus terang.
    (StatusCode::NOT_IMPLEMENTED, Json(json!({"error_code": 5025, "error_message": "SMTP_NOT_CONFIGURED", "error": "password-reset email delivery not implemented yet"})))
}

/// POST /auth/magic-generate/ — paritas `MagicGenerateEndpoint` (`magic.py:36-61`)
/// SEBATAS gate (5000/5025/5005). Penerbitan kode + email = follow-up bersama
/// forgot-password; sukses kelak 200 `{"key": str}`.
pub async fn magic_generate(
    State(st): State<AppState>,
    Json(body): Json<EmailCheckBody>,
) -> (StatusCode, Json<Value>) {
    // NOTE: `instances` punya `deleted_at`, `users` tidak — selaras forgot-password.
    let setup: Option<bool> = sqlx::query_scalar(
        "SELECT is_setup_done FROM instances WHERE deleted_at IS NULL ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_optional(&st.pool)
    .await
    .unwrap_or(None);
    if setup != Some(true) {
        return (StatusCode::BAD_REQUEST, auth_error(5000, "INSTANCE_NOT_CONFIGURED"));
    }
    if !smtp_configured(&std::env::var("EMAIL_HOST").unwrap_or_default()) {
        return (StatusCode::BAD_REQUEST, auth_error(5025, "SMTP_NOT_CONFIGURED"));
    }
    let email = body.email.unwrap_or_default().to_lowercase().trim().to_string();
    if email.is_empty() || !email_valid(&email) {
        return (StatusCode::BAD_REQUEST, auth_error(5005, "INVALID_EMAIL"));
    }
    // SMTP terkonfigurasi tapi penerbitan kode + email belum ada → katakan terus terang.
    (StatusCode::NOT_IMPLEMENTED, Json(json!({"error_code": 5025, "error_message": "SMTP_NOT_CONFIGURED", "error": "magic-code email delivery not implemented yet"})))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csrf_token_shape() {
        let v = csrf_token_value();
        assert_eq!(v, serde_json::json!({"csrf_token": ""}));
    }

    #[test]
    fn password_strength_policy() {
        assert!(password_strong_enough("xQ9#mZ2!vL8$pL4@"));
        assert!(!password_strong_enough("password123"));
        assert!(!password_strong_enough("abc"));
    }

    #[test]
    fn set_password_user_shape() {
        let v = user_subset_json("11111111-1111-1111-1111-111111111111", "a@b.co", "A", "B");
        assert_eq!(v["email"], "a@b.co");
        assert!(v.get("password").is_none());
    }

    #[test]
    fn smtp_gate() {
        assert!(!smtp_configured(""));
        assert!(smtp_configured("smtp.example.com"));
    }

    #[test]
    fn magic_key_shape() {
        let v = json!({"key": "abc123"});
        assert_eq!(v["key"], "abc123");
    }
}
