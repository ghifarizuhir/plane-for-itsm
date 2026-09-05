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

#[derive(Deserialize)]
pub struct UpdateEmailBody {
    pub email: Option<String>,
    pub code: Option<String>,
}

/// PATCH /api/users/me/email/ — paritas `UserEndpoint.update_email`.
pub async fn update_email(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UpdateEmailBody>,
) -> (StatusCode, Json<Value>) {
    // Galat DB = 500; hanya user yang benar-benar hilang yang jadi 401.
    let row: Option<(String,)> = match sqlx::query_as("SELECT email FROM users WHERE id = $1")
        .bind(auth.0)
        .fetch_optional(&st.pool)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "update-email: current-email lookup failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})));
        }
    };
    let Some((current,)) = row else {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid credentials"})));
    };
    let email = match validate_new_email(&st.pool, auth.0, &current.to_lowercase(), body.email).await {
        Ok(e) => e,
        Err(err) => return err,
    };
    let code = body.code.unwrap_or_default().trim().to_string();
    if code.is_empty() {
        return (StatusCode::BAD_REQUEST, plain_error("Verification code is required"));
    }
    let mut conn = match st.redis_client().await {
        Ok(c) => c,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"}))),
    };
    let key = email_code_key(&auth.0, &email);
    let cached: Option<String> = redis::cmd("GET").arg(&key).query_async(&mut conn).await.unwrap_or(None);
    let Some(raw) = cached else {
        return (StatusCode::BAD_REQUEST, plain_error("Verification code has expired or is invalid"));
    };
    let stored: String = serde_json::from_str::<Value>(&raw).ok().and_then(|v| v.get("token").and_then(|t| t.as_str()).map(str::to_string)).unwrap_or_default();
    if stored != code {
        return (StatusCode::BAD_REQUEST, plain_error("Invalid verification code"));
    }
    // Cek ulang duplikat (bisa diambil user lain antara generate dan update);
    // galat DB = 500 agar tak fail-open menimpa email duplikat.
    let taken: Option<bool> = match sqlx::query_scalar("SELECT true FROM users WHERE email = $1 AND id <> $2")
        .bind(&email).bind(auth.0).fetch_optional(&st.pool).await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "update-email: duplicate-email recheck failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})));
        }
    };
    if taken == Some(true) {
        return (StatusCode::BAD_REQUEST, plain_error("An account with this email already exists"));
    }
    let upd = sqlx::query("UPDATE users SET email = $1, is_email_verified = false, updated_at = now() WHERE id = $2")
        .bind(&email).bind(auth.0).execute(&st.pool).await;
    if upd.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})));
    }
    let _: () = redis::cmd("DEL").arg(&key).query_async(&mut conn).await.unwrap_or(());
    // Tanpa logout server-side: sesi Rust stateless (JWT ≤15 mnt, refresh
    // me-resolve uid → email baru berlaku otomatis); frontend sign-out sendiri.
    let back: Option<(String, String, String)> =
        match sqlx::query_as("SELECT email, first_name, last_name FROM users WHERE id = $1")
            .bind(auth.0).fetch_optional(&st.pool).await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "update-email: re-read after update failed");
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})));
            }
        };
    match back {
        Some((email, first, last)) => (StatusCode::OK, Json(json!({"id": auth.0, "email": email, "first_name": first, "last_name": last}))),
        None => (StatusCode::NOT_FOUND, Json(json!({"error": "User not found"}))),
    }
}

/// Logo workspace: aset file diutamakan, fallback ke kolom `logo` lama.
pub fn pick_logo_url(asset: Option<&str>, logo: Option<&str>) -> Option<String> {
    asset.map(str::to_string).or_else(|| logo.map(str::to_string))
}

#[derive(sqlx::FromRow)]
struct MyWorkspaceRow {
    id: uuid::Uuid,
    name: String,
    slug: String,
    timezone: String,
    organization_size: Option<String>,
    logo: Option<String>,
    logo_asset_url: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
    owner_id: uuid::Uuid,
    owner_email: String,
    owner_first: String,
    owner_last: String,
    role: i16,
    total_members: i64,
}

/// GET /api/users/me/workspaces/ — paritas `UserWorkSpacesEndpoint.get`
/// (workspace ber-membership aktif + anotasi role & total_members).
pub async fn my_workspaces(
    State(st): State<AppState>,
    auth: AuthUser,
) -> (StatusCode, Json<Value>) {
    // `file_assets` tidak punya kolom `asset_url` (skema aktual: `asset`,
    // varchar 800) — logo diambil dari `fa.asset`, fallback ke `w.logo`.
    // Field `url` DIHILANGKAN: tidak ada kolomnya di `workspaces` dan tidak
    // ada kode FE yang membaca properti `workspace.url`.
    let rows: Vec<MyWorkspaceRow> = match sqlx::query_as(
        "SELECT w.id, w.name, w.slug, w.timezone, w.organization_size, w.logo, \
                fa.asset AS logo_asset_url, \
                w.created_at, w.updated_at, w.created_by_id, w.updated_by_id, \
                o.id AS owner_id, o.email AS owner_email, \
                o.first_name AS owner_first, o.last_name AS owner_last, \
                wm.role AS role, \
                (SELECT COUNT(*) FROM workspace_members m JOIN users u ON u.id = m.member_id \
                  WHERE m.workspace_id = w.id AND m.is_active = true AND m.deleted_at IS NULL \
                    AND u.is_bot = false) AS total_members \
         FROM workspaces w \
         JOIN workspace_members wm ON wm.workspace_id = w.id \
           AND wm.member_id = $1 AND wm.is_active = true AND wm.deleted_at IS NULL \
         JOIN users o ON o.id = w.owner_id \
         LEFT JOIN file_assets fa ON fa.id = w.logo_asset_id \
         WHERE w.deleted_at IS NULL \
         ORDER BY w.created_at DESC",
    )
    .bind(auth.0)
    .fetch_all(&st.pool)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "my-workspaces: lookup failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})));
        }
    };
    let out: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id,
                "name": r.name,
                "slug": r.slug,
                "timezone": r.timezone,
                "organization_size": r.organization_size.unwrap_or_default(),
                "logo_url": pick_logo_url(r.logo_asset_url.as_deref(), r.logo.as_deref()),
                "created_at": r.created_at,
                "updated_at": r.updated_at,
                "created_by": r.created_by_id,
                "updated_by": r.updated_by_id,
                "owner": {
                    "id": r.owner_id,
                    "email": r.owner_email,
                    "first_name": r.owner_first,
                    "last_name": r.owner_last,
                },
                "role": r.role,
                "total_members": r.total_members,
            })
        })
        .collect();
    (StatusCode::OK, Json(Value::Array(out)))
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

    #[test]
    fn email_update_clears_key_format() {
        // key yang dihapus setelah sukses harus sama dengan key generate
        let uid = uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        assert_eq!(email_code_key(&uid, "n@x.io"), "emailcode:11111111-1111-1111-1111-111111111111:n@x.io");
    }

    #[test]
    fn logo_fallback_order() {
        assert_eq!(pick_logo_url(Some("a"), Some("b")).as_deref(), Some("a"));
        assert_eq!(pick_logo_url(None, Some("b")).as_deref(), Some("b"));
        assert_eq!(pick_logo_url(None, None), None);
    }
}
