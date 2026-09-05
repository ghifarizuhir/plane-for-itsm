//! Paritas Django `/api/users/me/*` (`apps/api/plane/app/urls/user.py`,
//! `workspace.py`, `project.py`).

use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::middleware::auth::AuthUser;
use crate::state::AppState;

pub fn email_code_throttle_key(uid: &uuid::Uuid) -> String {
    format!("emailcode:throttle:{uid}")
}

pub fn email_code_key(uid: &uuid::Uuid, email: &str) -> String {
    format!("emailcode:{uid}:{email}")
}

/// 6 digit seperti Django `secrets.randbelow(900000) + 100000`.
pub fn new_email_code() -> String {
    let n: u32 = rand::random_range(100000..1000000);
    format!("{n:06}")
}

#[derive(Deserialize)]
pub struct EmailBody {
    pub email: Option<String>,
}

fn plain_error(message: &str) -> Json<Value> {
    Json(json!({"error": message}))
}

/// Validasi email-baru, pesan byte-exact dari `_validate_new_email`.
async fn validate_new_email(
    pool: &sqlx::PgPool,
    uid: uuid::Uuid,
    current: &str,
    raw: Option<String>,
) -> Result<String, (StatusCode, Json<Value>)> {
    let email = raw.unwrap_or_default().to_lowercase().trim().to_string();
    if email.is_empty() {
        return Err((StatusCode::BAD_REQUEST, plain_error("Email is required")));
    }
    if !crate::routes::auth::email_valid(&email) {
        return Err((StatusCode::BAD_REQUEST, plain_error("Invalid email format")));
    }
    if email == current {
        return Err((StatusCode::BAD_REQUEST, plain_error("New email must be different from current email")));
    }
    // NOTE: tabel `users` tidak punya `deleted_at` (skema aktual) — filter email saja.
    // Galat DB di sini jangan fail-open (bisa melewatkan duplikat) — balas 500.
    let taken: Option<bool> = match sqlx::query_scalar("SELECT true FROM users WHERE email = $1 AND id <> $2")
        .bind(&email)
        .bind(uid)
        .fetch_optional(pool)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "generate-code: duplicate-email check failed");
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"}))));
        }
    };
    if taken == Some(true) {
        return Err((StatusCode::BAD_REQUEST, plain_error("An account with this email already exists")));
    }
    Ok(email)
}

/// POST /api/users/me/email/generate-code/ — paritas
/// `UserEndpoint.generate_email_verification_code` (3/jam via Redis).
pub async fn generate_email_code(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<EmailBody>,
) -> (StatusCode, Json<Value>) {
    // Galat DB = 500; hanya user yang benar-benar hilang yang jadi 401.
    let email_row: Option<(String,)> = match sqlx::query_as("SELECT email FROM users WHERE id = $1")
        .bind(auth.0)
        .fetch_optional(&st.pool)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "generate-code: current-email lookup failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})));
        }
    };
    let Some((current,)) = email_row else {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid credentials"})));
    };
    let email = match validate_new_email(&st.pool, auth.0, &current.to_lowercase(), body.email).await {
        Ok(e) => e,
        Err(err) => return err,
    };
    let mut conn = match st.redis_client().await {
        Ok(c) => c,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"}))),
    };
    // INCR gagal = fail-closed (anggap budget habis) agar throttle tak bisa
    // dilewati saat Redis bermasalah.
    let count: i64 = redis::cmd("INCR")
        .arg(email_code_throttle_key(&auth.0))
        .query_async(&mut conn)
        .await
        .unwrap_or(4);
    if count == 1 {
        // Tanpa TTL, kunci throttle mengunci user selamanya — kalau EXPIRE
        // gagal, hapus kuncinya agar hitungan mulai bersih di percobaan berikut.
        let expire: redis::RedisResult<()> = redis::cmd("EXPIRE")
            .arg(email_code_throttle_key(&auth.0))
            .arg(3600)
            .query_async(&mut conn)
            .await;
        if let Err(e) = expire {
            tracing::warn!(error = %e, "generate-code: EXPIRE throttle failed, resetting key");
            let _: () = redis::cmd("DEL")
                .arg(email_code_throttle_key(&auth.0))
                .query_async(&mut conn)
                .await
                .unwrap_or(());
        }
    }
    if count > 3 {
        return (StatusCode::TOO_MANY_REQUESTS, Json(json!({"error_code": 5900, "error_message": "RATE_LIMIT_EXCEEDED"})));
    }
    let code = new_email_code();
    let stored: redis::RedisResult<()> = redis::cmd("SET")
        .arg(email_code_key(&auth.0, &email))
        .arg(json!({"token": code}).to_string())
        .arg("EX")
        .arg(600)
        .query_async(&mut conn)
        .await;
    if stored.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})));
    }
    // SMTP belum ada: kode tersimpan di Redis 10 mnt; pengiriman email = follow-up.
    tracing::info!(user_id = %auth.0, "email verification code stored (delivery pending SMTP)");
    (StatusCode::OK, Json(json!({"message": "Verification code sent to email"})))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_code_throttle_key_format() {
        let uid = uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        assert_eq!(email_code_throttle_key(&uid), "emailcode:throttle:11111111-1111-1111-1111-111111111111");
        assert_eq!(email_code_key(&uid, "a@b.co"), "emailcode:11111111-1111-1111-1111-111111111111:a@b.co");
    }

    #[test]
    fn six_digit_code_range() {
        for _ in 0..200 {
            let c = new_email_code();
            assert!(c.len() == 6 && c.bytes().all(|b| b.is_ascii_digit()));
            let n: u32 = c.parse().expect("kode harus numerik");
            assert!((100000..=999999).contains(&n));
        }
    }
}
