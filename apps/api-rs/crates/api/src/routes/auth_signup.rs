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

use axum::{http::StatusCode, Json};
use serde_json::{json, Value};

// Kode error Django (`adapter/error.py`).
pub const INSTANCE_NOT_CONFIGURED: i32 = 5000;
pub const SIGNUP_DISABLED: i32 = 5015;
pub const PASSWORD_TOO_WEAK: i32 = 5021;
pub const USER_ALREADY_EXIST: i32 = 5030;
pub const REQUIRED_EMAIL_PASSWORD_SIGN_UP: i32 = 5040;
pub const INVALID_EMAIL_SIGN_UP: i32 = 5045;

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

/// `if not email or not password` Django pada nilai RAW (sebelum strip):
/// string berisi spasi dianggap terisi → lolos ke validasi email berikutnya.
pub fn signup_fields_missing(email_raw: &str, password_raw: &str) -> bool {
    email_raw.is_empty() || password_raw.is_empty()
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
}
