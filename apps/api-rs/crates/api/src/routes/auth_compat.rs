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
}
