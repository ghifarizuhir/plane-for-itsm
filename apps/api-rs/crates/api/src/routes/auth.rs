// crates/api/src/routes/auth.rs
//
// Session auth: `POST /api/auth/login|refresh|logout/`.
//
// - login: email+password lawan hash Django → 200 `{id, email}` + 2 Set-Cookie
//   (access JWT 15 mnt, refresh opaque 30 hari di Redis).
// - refresh: baca cookie rt → rotasi (hapus hash lama, terbit pasangan baru).
//   Hash tak dikenal → 401. Keluarga refresh dilacak via secondary index
//   `auth:family:{family}` (SET member hash) untuk revoke per-keluarga.
// - logout: hapus hash refresh + clear kedua cookie → 200.
//
// 401 generik selalu `{"error": ...}`.
use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::state::AppState;
use common::auth as authn;

const ACCESS_TTL_SECS: i64 = 900;
const REFRESH_TTL_SECS: i64 = 30 * 24 * 3600;

#[derive(Deserialize)]
pub struct LoginBody {
    pub email: String,
    pub password: String,
}

fn cookie_pair(secure: bool) -> (&'static str, &'static str) {
    if secure {
        ("__Host-plane_at", "__Host-plane_rt")
    } else {
        ("plane_at", "plane_rt")
    }
}

fn set_cookies(headers: &mut HeaderMap, at: &str, rt: &str, secure: bool) {
    let (at_name, rt_name) = cookie_pair(secure);
    headers.append(
        header::SET_COOKIE,
        authn::cookie_headers(at_name, at, ACCESS_TTL_SECS, secure)
            .parse()
            .unwrap(),
    );
    headers.append(
        header::SET_COOKIE,
        authn::cookie_headers(rt_name, rt, REFRESH_TTL_SECS, secure)
            .parse()
            .unwrap(),
    );
}

fn clear_cookies(headers: &mut HeaderMap, secure: bool) {
    let (at_name, rt_name) = cookie_pair(secure);
    headers.append(
        header::SET_COOKIE,
        authn::clear_cookie_header(at_name, secure).parse().unwrap(),
    );
    headers.append(
        header::SET_COOKIE,
        authn::clear_cookie_header(rt_name, secure).parse().unwrap(),
    );
}

/// Baca cookie mentah dari header (tanpa dependensi cookie-jar):
/// terima nama polos maupun `__Host-` terlepas dari flag secure.
fn read_cookie(headers: &HeaderMap, names: &[&str]) -> Option<String> {
    let cookies = headers.get("cookie")?.to_str().ok()?;
    for pair in cookies.split(';') {
        let (k, v) = pair.trim().split_once('=')?;
        if names.contains(&k) {
            return Some(v.trim().to_string());
        }
    }
    None
}

fn family_key(family: &str) -> String {
    format!("auth:family:{family}")
}

/// POST /api/auth/login/ — email+password lawan hash Django.
pub async fn login(
    State(st): State<AppState>,
    Json(body): Json<LoginBody>,
) -> Result<(StatusCode, HeaderMap, Json<Value>), common::errors::AppError> {
    let email = body.email.to_lowercase().trim().to_string();
    // NOTE: tabel `users` tidak punya `deleted_at` (skema aktual) — filter email saja.
    let row: Option<(uuid::Uuid, String, String)> =
        sqlx::query_as("SELECT id, email, password FROM users WHERE email = $1")
            .bind(&email)
            .fetch_optional(&st.pool)
            .await?;
    let Some((uid, db_email, hash)) = row else {
        return Ok((
            StatusCode::UNAUTHORIZED,
            Default::default(),
            Json(json!({"error": "invalid credentials"})),
        ));
    };
    if !authn::verify_django_password(&body.password, &hash) {
        return Ok((
            StatusCode::UNAUTHORIZED,
            Default::default(),
            Json(json!({"error": "invalid credentials"})),
        ));
    }
    let access = authn::encode_access(&uid, &st.config.jwt_secret, ACCESS_TTL_SECS);
    let (hash_rt, raw_rt) = authn::new_refresh();
    let family = uuid::Uuid::new_v4().to_string();
    let mut conn = st.redis_client().await.map_err(|e| anyhow::anyhow!(e))?;
    redis::cmd("SET")
        .arg(authn::refresh_key(&hash_rt))
        .arg(format!("{uid}:{family}"))
        .arg("EX")
        .arg(REFRESH_TTL_SECS)
        .query_async::<()>(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    redis::cmd("SADD")
        .arg(family_key(&family))
        .arg(&hash_rt)
        .query_async::<()>(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    redis::cmd("EXPIRE")
        .arg(family_key(&family))
        .arg(REFRESH_TTL_SECS)
        .query_async::<()>(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let mut headers = HeaderMap::new();
    set_cookies(&mut headers, &access, &raw_rt, st.config.cookie_secure);
    Ok((StatusCode::OK, headers, Json(json!({"id": uid, "email": db_email}))))
}

/// POST /api/auth/refresh/ — rotasi pasangan token dari cookie refresh.
///
/// Hash tak dikenal (reuse/palsu) → 401. Revoke keluarga penuh tidak mungkin
/// dari hash tak dikenal (family hanya tersimpan di value Redis), jadi reuse
/// ditolak tanpa efek samping; anggota keluarga yang sah tetap berlaku hingga
/// dipakai (rotasi) atau logout.
pub async fn refresh(
    State(st): State<AppState>,
    headers_in: HeaderMap,
) -> Result<(StatusCode, HeaderMap, Json<Value>), common::errors::AppError> {
    let unauthorized = || {
        (
            StatusCode::UNAUTHORIZED,
            HeaderMap::new(),
            Json(json!({"error": "invalid refresh token"})),
        )
    };
    let Some(raw) = read_cookie(&headers_in, &["plane_rt", "__Host-plane_rt"]) else {
        return Ok(unauthorized());
    };
    let old_hash = authn::sha256hex(&raw);
    let mut conn = st.redis_client().await.map_err(|e| anyhow::anyhow!(e))?;
    let val: Option<String> = redis::cmd("GET")
        .arg(authn::refresh_key(&old_hash))
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let Some(val) = val else {
        return Ok(unauthorized());
    };
    let (uid_str, family) = val.split_once(':').unwrap_or(("", ""));
    let uid: uuid::Uuid = uid_str.parse().map_err(|_| anyhow::anyhow!("bad refresh value"))?;
    // Hapus hash lama + keluarkan dari SET keluarga.
    redis::cmd("DEL")
        .arg(authn::refresh_key(&old_hash))
        .query_async::<()>(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    redis::cmd("SREM")
        .arg(family_key(family))
        .arg(&old_hash)
        .query_async::<()>(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let row: Option<(String,)> = sqlx::query_as("SELECT email FROM users WHERE id = $1")
        .bind(uid)
        .fetch_optional(&st.pool)
        .await?;
    let Some((db_email,)) = row else {
        return Ok(unauthorized());
    };
    // Terbit pasangan baru dalam keluarga yang sama.
    let access = authn::encode_access(&uid, &st.config.jwt_secret, ACCESS_TTL_SECS);
    let (hash_rt, raw_rt) = authn::new_refresh();
    redis::cmd("SET")
        .arg(authn::refresh_key(&hash_rt))
        .arg(format!("{uid}:{family}"))
        .arg("EX")
        .arg(REFRESH_TTL_SECS)
        .query_async::<()>(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    redis::cmd("SADD")
        .arg(family_key(family))
        .arg(&hash_rt)
        .query_async::<()>(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    redis::cmd("EXPIRE")
        .arg(family_key(family))
        .arg(REFRESH_TTL_SECS)
        .query_async::<()>(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let mut headers = HeaderMap::new();
    set_cookies(&mut headers, &access, &raw_rt, st.config.cookie_secure);
    Ok((StatusCode::OK, headers, Json(json!({"id": uid, "email": db_email}))))
}

/// POST /api/auth/logout/ — hapus hash refresh + clear kedua cookie → 200.
/// Idempoten: tanpa cookie pun tetap 200 + header clear.
pub async fn logout(
    State(st): State<AppState>,
    headers_in: HeaderMap,
) -> Result<(StatusCode, HeaderMap, Json<Value>), common::errors::AppError> {
    if let Some(raw) = read_cookie(&headers_in, &["plane_rt", "__Host-plane_rt"]) {
        let hash = authn::sha256hex(&raw);
        let mut conn = st.redis_client().await.map_err(|e| anyhow::anyhow!(e))?;
        let val: Option<String> = redis::cmd("GET")
            .arg(authn::refresh_key(&hash))
            .query_async(&mut conn)
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        redis::cmd("DEL")
            .arg(authn::refresh_key(&hash))
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        if let Some(val) = val {
            if let Some((_, family)) = val.split_once(':') {
                redis::cmd("SREM")
                    .arg(family_key(family))
                    .arg(&hash)
                    .query_async::<()>(&mut conn)
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?;
            }
        }
    }
    let mut headers = HeaderMap::new();
    clear_cookies(&mut headers, st.config.cookie_secure);
    Ok((StatusCode::OK, headers, Json(json!({"message": "Logged out"}))))
}
