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
    extract::{Host, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::Redirect,
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

// ---------------------------------------------------------------------------
// OAuth login via GitHub / Google.
//
// - `GET /api/auth/oauth/:provider/start/?next_path=` menyimpan state acak di
//   Redis (`auth:oauth:{state}` = `{provider}:{next_path}`, EX 600 dtk) lalu
//   302 ke halaman otorisasi provider.
// - `GET /api/auth/oauth/:provider/callback/?code=&state=` menukar code jadi
//   email terverifikasi, find-or-create user, terbit cookie sesi seperti
//   login, lalu 302 ke `next_path`.
//
// Semua kegagalan di alur callback (state hilang, exchange gagal, DB/Redis
// down) me-redirect ke `{frontend}/sign-in?error=oauth` — bukan 500 — karena
// ini alur navigasi browser dan frontend menangani query `error=oauth`.
// Deviasi dari sketsa plan (disengaja, meniru Django
// `plane/authentication/provider/oauth/*.py`):
// - `auth_url`/`exchange` membawa `redirect_uri` (`{scheme}://{host}/api/auth/
//   oauth/{provider}/callback/`, host dari header Host, scheme dari
//   `X-Forwarded-Proto` atau flag `COOKIE_SECURE`). Tanpa ini Google menolak
//   token exchange (`redirect_uri` wajib) dan GitHub callback tak cocok.
// - `next_path` disanitasi (hanya path absolut `/...`, bukan `//...`);
//   plan menyimpan mentah (risiko open-redirect).
const OAUTH_STATE_TTL_SECS: i64 = 600;

fn oauth_state_key(state: &str) -> String {
    format!("auth:oauth:{state}")
}

/// `next_path` aman untuk header Location: hanya path absolut (tak boleh
/// `//host`, URL absolut, string kosong, atau karakter kontrol).
fn sanitize_next_path(raw: Option<&str>) -> String {
    let fallback = "/".to_string();
    let Some(raw) = raw else {
        return fallback;
    };
    if !raw.starts_with('/') || raw.starts_with("//") {
        return fallback;
    }
    if raw.bytes().any(|b| b < 0x20 || b == 0x7f) {
        return fallback;
    }
    if axum::http::HeaderValue::from_str(raw).is_err() {
        return fallback;
    }
    raw.to_string()
}

/// Percent-encode nilai query (RFC 3986 unreserved dibiarkan) — tanpa dep baru.
fn pct_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// 302 yang tak pernah panic walau URL tak valid (jatuh ke `/`).
fn safe_redirect(url: String) -> Redirect {
    if axum::http::HeaderValue::from_str(&url).is_ok() {
        Redirect::to(&url)
    } else {
        tracing::warn!("oauth redirect URL invalid, falling back to /");
        Redirect::to("/")
    }
}

fn frontend_base(config: &common::config::AppConfig) -> String {
    config.frontend_url.trim_end_matches('/').to_string()
}

fn oauth_error_redirect(config: &common::config::AppConfig) -> Redirect {
    safe_redirect(format!("{}/sign-in?error=oauth", frontend_base(config)))
}

fn oauth_disabled_redirect(config: &common::config::AppConfig) -> Redirect {
    safe_redirect(format!("{}/?error=oauth_disabled", frontend_base(config)))
}

/// `provider:next_path` yang disimpan di Redis → (provider, next_path).
/// `next_path` boleh mengandung ':' sehingga split di ':' pertama.
fn parse_oauth_state(stored: &str) -> Option<(String, String)> {
    let (provider, next) = stored.split_once(':')?;
    if provider.is_empty() {
        return None;
    }
    Some((provider.to_string(), sanitize_next_path(Some(next))))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ProviderKind {
    Github,
    Google,
}

fn provider_kind(slug: &str) -> Option<ProviderKind> {
    match slug {
        "github" => Some(ProviderKind::Github),
        "google" => Some(ProviderKind::Google),
        _ => None,
    }
}

fn provider_slug(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Github => "github",
        ProviderKind::Google => "google",
    }
}

fn provider_creds(config: &common::config::AppConfig, kind: ProviderKind) -> (String, String) {
    match kind {
        ProviderKind::Github => (
            config.github_client_id.clone(),
            config.github_client_secret.clone(),
        ),
        ProviderKind::Google => (
            config.google_client_id.clone(),
            config.google_client_secret.clone(),
        ),
    }
}

/// Scheme publik API untuk `redirect_uri`: hormati `X-Forwarded-Proto` dari
/// reverse proxy, lalu tebak dari `COOKIE_SECURE` (https bila cookie secure).
fn public_scheme(headers: &HeaderMap, config: &common::config::AppConfig) -> &'static str {
    if let Some(proto) = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
    {
        let first = proto.split(',').next().unwrap_or("").trim();
        if first.eq_ignore_ascii_case("https") {
            return "https";
        }
        if first.eq_ignore_ascii_case("http") {
            return "http";
        }
    }
    if config.cookie_secure {
        "https"
    } else {
        "http"
    }
}

fn oauth_redirect_uri(
    headers: &HeaderMap,
    host: &str,
    config: &common::config::AppConfig,
    kind: ProviderKind,
) -> String {
    format!(
        "{}://{}/api/auth/oauth/{}/callback/",
        public_scheme(headers, config),
        host,
        provider_slug(kind)
    )
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

#[async_trait::async_trait]
pub trait OAuthProvider {
    fn auth_url(&self, state: &str, redirect_uri: &str) -> String;
    /// Tukar `code` otorisasi jadi email yang sudah terverifikasi provider.
    async fn exchange(&self, code: &str, redirect_uri: &str) -> Result<String, String>;
}

pub struct GithubProvider {
    client_id: String,
    client_secret: String,
    http: reqwest::Client,
}

pub struct GoogleProvider {
    client_id: String,
    client_secret: String,
    http: reqwest::Client,
}

#[derive(Deserialize)]
struct GithubTokenResp {
    access_token: Option<String>,
    error_description: Option<String>,
}

#[derive(Deserialize)]
struct GithubEmail {
    email: String,
    primary: Option<bool>,
    verified: Option<bool>,
}

#[derive(Deserialize)]
struct GoogleTokenResp {
    access_token: Option<String>,
    error_description: Option<String>,
}

#[derive(Deserialize)]
struct GoogleUserinfo {
    email: Option<String>,
    email_verified: Option<bool>,
}

#[async_trait::async_trait]
impl OAuthProvider for GithubProvider {
    fn auth_url(&self, state: &str, redirect_uri: &str) -> String {
        format!(
            "https://github.com/login/oauth/authorize?client_id={}&redirect_uri={}&scope=user:email&state={}",
            pct_encode(&self.client_id),
            pct_encode(redirect_uri),
            pct_encode(state),
        )
    }

    async fn exchange(&self, code: &str, redirect_uri: &str) -> Result<String, String> {
        let token: GithubTokenResp = self
            .http
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("code", code),
                ("redirect_uri", redirect_uri),
            ])
            .send()
            .await
            .map_err(|e| format!("github token request failed: {e}"))?
            .error_for_status()
            .map_err(|e| format!("github token rejected: {e}"))?
            .json()
            .await
            .map_err(|e| format!("github token decode failed: {e}"))?;
        let Some(access_token) = token.access_token else {
            return Err(token
                .error_description
                .unwrap_or_else(|| "github token exchange failed".to_string()));
        };
        // GitHub WAJIB dikirimi User-Agent, kalau tidak API menjawab 403.
        let emails: Vec<GithubEmail> = self
            .http
            .get("https://api.github.com/user/emails")
            .header("Accept", "application/json")
            .header("User-Agent", "plane-api-rs")
            .bearer_auth(&access_token)
            .send()
            .await
            .map_err(|e| format!("github emails request failed: {e}"))?
            .error_for_status()
            .map_err(|e| format!("github emails rejected: {e}"))?
            .json()
            .await
            .map_err(|e| format!("github emails decode failed: {e}"))?;
        // Syarat primary DAN verified — primary yang belum diverifikasi bisa
        // dipakai membajak akun (GHSA-7j95-vh8g-f365).
        emails
            .into_iter()
            .find(|e| e.primary == Some(true) && e.verified == Some(true))
            .map(|e| e.email)
            .ok_or_else(|| "no primary verified github email".to_string())
    }
}

#[async_trait::async_trait]
impl OAuthProvider for GoogleProvider {
    fn auth_url(&self, state: &str, redirect_uri: &str) -> String {
        format!(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=openid%20email&state={}",
            pct_encode(&self.client_id),
            pct_encode(redirect_uri),
            pct_encode(state),
        )
    }

    async fn exchange(&self, code: &str, redirect_uri: &str) -> Result<String, String> {
        let token: GoogleTokenResp = self
            .http
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("code", code),
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("redirect_uri", redirect_uri),
                ("grant_type", "authorization_code"),
            ])
            .send()
            .await
            .map_err(|e| format!("google token request failed: {e}"))?
            .error_for_status()
            .map_err(|e| format!("google token rejected: {e}"))?
            .json()
            .await
            .map_err(|e| format!("google token decode failed: {e}"))?;
        let Some(access_token) = token.access_token else {
            return Err(token
                .error_description
                .unwrap_or_else(|| "google token exchange failed".to_string()));
        };
        let info: GoogleUserinfo = self
            .http
            .get("https://openidconnect.googleapis.com/v1/userinfo")
            .bearer_auth(&access_token)
            .send()
            .await
            .map_err(|e| format!("google userinfo request failed: {e}"))?
            .error_for_status()
            .map_err(|e| format!("google userinfo rejected: {e}"))?
            .json()
            .await
            .map_err(|e| format!("google userinfo decode failed: {e}"))?;
        // Fail closed: email tanpa `email_verified=true` ditolak (hindari
        // klaim email sewenang-wenang, GHSA-7j95-vh8g-f365).
        match (info.email, info.email_verified) {
            (Some(email), Some(true)) => Ok(email),
            _ => Err("google email not verified".to_string()),
        }
    }
}

/// Cari user by email; bila belum ada, buat seperti pola Django
/// (`adapter/base.py`, `User.save`): username = uuid hex 32 char,
/// display_name = local-part email, password `'!'` (unusable ala Django),
/// `is_email_verified=true` karena email sudah diverifikasi provider.
/// NOTE: tabel `users` tidak punya `deleted_at` — filter email saja.
async fn find_or_create_user(pool: &sqlx::PgPool, email: &str) -> anyhow::Result<uuid::Uuid> {
    let email = email.to_lowercase().trim().to_string();
    if let Some((id,)) = sqlx::query_as::<_, (uuid::Uuid,)>("SELECT id FROM users WHERE email = $1")
        .bind(&email)
        .fetch_optional(pool)
        .await?
    {
        return Ok(id);
    }
    let username = uuid::Uuid::new_v4().simple().to_string();
    let display_name = email.split('@').next().unwrap_or(&email).to_string();
    let (id,): (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO users (id, email, username, password, first_name, last_name, display_name, avatar, date_joined, token, user_timezone, last_location, created_location, last_login_ip, last_logout_ip, last_login_medium, last_login_uagent, is_active, is_staff, is_superuser, is_managed, is_password_expired, is_email_verified, is_password_autoset, is_bot, is_email_valid, is_password_reset_required, created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, '!', '', '', $3, '', now(), '', 'UTC', '', '', '', '', 'oauth', '', true, false, false, false, false, true, false, false, true, false, now(), now()) RETURNING id",
    )
    .bind(&email)
    .bind(&username)
    .bind(&display_name)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Terbit pasangan token sesi (access JWT + refresh opaque di Redis),
/// dipakai callback OAuth — sama seperti login.
async fn issue_session(st: &AppState, uid: &uuid::Uuid) -> anyhow::Result<(String, String)> {
    let access = authn::encode_access(uid, &st.config.jwt_secret, ACCESS_TTL_SECS);
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
    Ok((access, raw_rt))
}

#[derive(Deserialize)]
pub struct OAuthStartQuery {
    pub next_path: Option<String>,
}

#[derive(Deserialize)]
pub struct OAuthCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
}

/// GET /api/auth/oauth/:provider/start/ — simpan state lalu 302 ke provider.
pub async fn oauth_start(
    State(st): State<AppState>,
    Path(provider): Path<String>,
    Host(host): Host,
    headers: HeaderMap,
    Query(q): Query<OAuthStartQuery>,
) -> (HeaderMap, Redirect) {
    let Some(kind) = provider_kind(&provider) else {
        return (HeaderMap::new(), oauth_error_redirect(&st.config));
    };
    let (client_id, client_secret) = provider_creds(&st.config, kind);
    if client_id.is_empty() || client_secret.is_empty() {
        return (HeaderMap::new(), oauth_disabled_redirect(&st.config));
    }
    let redirect_uri = oauth_redirect_uri(&headers, &host, &st.config, kind);
    let state = uuid::Uuid::new_v4().simple().to_string();
    let next = sanitize_next_path(q.next_path.as_deref());
    let value = format!("{}:{next}", provider_slug(kind));
    let mut conn = match st.redis_client().await {
        Ok(conn) => conn,
        Err(e) => {
            tracing::warn!(error=%e, "oauth start: redis unavailable");
            return (HeaderMap::new(), oauth_error_redirect(&st.config));
        }
    };
    if let Err(e) = redis::cmd("SET")
        .arg(oauth_state_key(&state))
        .arg(&value)
        .arg("EX")
        .arg(OAUTH_STATE_TTL_SECS)
        .query_async::<()>(&mut conn)
        .await
    {
        tracing::warn!(error=%e, "oauth start: state store failed");
        return (HeaderMap::new(), oauth_error_redirect(&st.config));
    }
    let http = http_client();
    let url = match kind {
        ProviderKind::Github => GithubProvider {
            client_id,
            client_secret,
            http,
        }
        .auth_url(&state, &redirect_uri),
        ProviderKind::Google => GoogleProvider {
            client_id,
            client_secret,
            http,
        }
        .auth_url(&state, &redirect_uri),
    };
    (HeaderMap::new(), safe_redirect(url))
}

/// GET /api/auth/oauth/:provider/callback/ — tukar code → sesi → 302 next.
pub async fn oauth_callback(
    State(st): State<AppState>,
    Path(provider): Path<String>,
    Host(host): Host,
    headers: HeaderMap,
    Query(q): Query<OAuthCallbackQuery>,
) -> (HeaderMap, Redirect) {
    let fail = || (HeaderMap::new(), oauth_error_redirect(&st.config));
    let Some(kind) = provider_kind(&provider) else {
        return fail();
    };
    let (Some(code), Some(state)) = (q.code.as_deref(), q.state.as_deref()) else {
        return fail();
    };
    if code.is_empty() || state.is_empty() {
        return fail();
    }
    let mut conn = match st.redis_client().await {
        Ok(conn) => conn,
        Err(e) => {
            tracing::warn!(error=%e, "oauth callback: redis unavailable");
            return fail();
        }
    };
    let stored: Option<String> = match redis::cmd("GETDEL")
        .arg(oauth_state_key(state))
        .query_async(&mut conn)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error=%e, "oauth callback: state lookup failed");
            return fail();
        }
    };
    let Some(stored) = stored else {
        return fail();
    };
    let (stored_provider, next) = match parse_oauth_state(&stored) {
        Some(v) => v,
        None => return fail(),
    };
    // Tolak state lintas-provider (state github tak bisa dipakai di google).
    if stored_provider != provider_slug(kind) {
        return fail();
    }
    let (client_id, client_secret) = provider_creds(&st.config, kind);
    if client_id.is_empty() || client_secret.is_empty() {
        return fail();
    }
    let redirect_uri = oauth_redirect_uri(&headers, &host, &st.config, kind);
    let http = http_client();
    let email = match kind {
        ProviderKind::Github => {
            GithubProvider {
                client_id,
                client_secret,
                http,
            }
            .exchange(code, &redirect_uri)
            .await
        }
        ProviderKind::Google => {
            GoogleProvider {
                client_id,
                client_secret,
                http,
            }
            .exchange(code, &redirect_uri)
            .await
        }
    };
    let email = match email {
        Ok(email) => email,
        Err(e) => {
            tracing::warn!("oauth callback: provider exchange failed: {e}");
            return fail();
        }
    };
    let uid = match find_or_create_user(&st.pool, &email).await {
        Ok(uid) => uid,
        Err(e) => {
            tracing::warn!(error=%e, "oauth callback: find_or_create_user failed");
            return fail();
        }
    };
    let (access, raw_rt) = match issue_session(&st, &uid).await {
        Ok(pair) => pair,
        Err(e) => {
            tracing::warn!(error=%e, "oauth callback: session issue failed");
            return fail();
        }
    };
    let mut out = HeaderMap::new();
    set_cookies(&mut out, &access, &raw_rt, st.config.cookie_secure);
    (out, safe_redirect(next))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, routing::get, Router};
    use tower::ServiceExt as _;

    fn test_config(client_id: &str, client_secret: &str) -> common::config::AppConfig {
        common::config::AppConfig {
            database_url: "postgres://plane:plane@127.0.0.1:5432/plane_test".into(),
            redis_url: std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://127.0.0.1:6379".into()),
            port: 8001,
            jwt_secret: "test-secret".into(),
            cookie_secure: false,
            frontend_url: "http://web:3000".into(),
            github_client_id: client_id.into(),
            github_client_secret: client_secret.into(),
            google_client_id: client_id.into(),
            google_client_secret: client_secret.into(),
        }
    }

    fn test_state(config: common::config::AppConfig) -> AppState {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy(&config.database_url)
            .expect("lazy pool");
        let redis = redis::Client::open(config.redis_url.as_str()).expect("redis client open");
        AppState {
            pool,
            redis,
            config,
        }
    }

    fn oauth_router(state: AppState) -> Router {
        Router::new()
            .route("/api/auth/oauth/:provider/start/", get(oauth_start))
            .route("/api/auth/oauth/:provider/callback/", get(oauth_callback))
            .with_state(state)
    }

    fn location_of(resp: &axum::response::Response) -> String {
        resp.headers()
            .get("location")
            .expect("redirect must carry Location")
            .to_str()
            .unwrap()
            .to_string()
    }

    #[test]
    fn sanitize_next_path_shapes() {
        assert_eq!(sanitize_next_path(None), "/");
        assert_eq!(sanitize_next_path(Some("/a/b?c=d")), "/a/b?c=d");
        assert_eq!(sanitize_next_path(Some("//evil.com/x")), "/");
        assert_eq!(sanitize_next_path(Some("https://evil.com/")), "/");
        assert_eq!(sanitize_next_path(Some("")), "/");
        assert_eq!(sanitize_next_path(Some("/a\nb")), "/");
        assert_eq!(sanitize_next_path(Some("relative/path")), "/");
    }

    #[test]
    fn oauth_state_roundtrip() {
        let (provider, next) = parse_oauth_state("github:/foo/bar").unwrap();
        assert_eq!(provider, "github");
        assert_eq!(next, "/foo/bar");
        // next_path boleh mengandung ':' — split di ':' pertama.
        let (provider, next) = parse_oauth_state("google:/x:y").unwrap();
        assert_eq!(provider, "google");
        assert_eq!(next, "/x:y");
        assert!(parse_oauth_state("no-colon").is_none());
        assert!(parse_oauth_state(":").is_none());
    }

    #[test]
    fn provider_auth_urls_carry_state_and_redirect() {
        let http = reqwest::Client::new();
        let gh = GithubProvider {
            client_id: "cid".into(),
            client_secret: "cs".into(),
            http: http.clone(),
        };
        let url = gh.auth_url("ST8", "http://api:8001/api/auth/oauth/github/callback/");
        assert!(url.starts_with("https://github.com/login/oauth/authorize?"));
        assert!(url.contains("client_id=cid"));
        assert!(url.contains("state=ST8"));
        assert!(url.contains("redirect_uri=http%3A%2F%2Fapi%3A8001"));

        let gg = GoogleProvider {
            client_id: "gcid".into(),
            client_secret: "gcs".into(),
            http,
        };
        let url = gg.auth_url("ST9", "https://api/x/callback/");
        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("scope=openid%20email"));
        assert!(url.contains("state=ST9"));
        assert!(url.contains("redirect_uri=https%3A%2F%2Fapi%2Fx%2Fcallback%2F"));
    }

    #[tokio::test]
    async fn start_unknown_provider_redirects_oauth_error() {
        let app = oauth_router(test_state(test_config("id", "secret")));
        let req = Request::builder()
            .uri("/api/auth/oauth/gitlab/start/")
            .header("host", "api.test")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert!(resp.status().is_redirection());
        assert_eq!(location_of(&resp), "http://web:3000/sign-in?error=oauth");
    }

    #[tokio::test]
    async fn start_empty_creds_redirects_oauth_disabled() {
        // Tanpa client_id/secret → 302 `/?error=oauth_disabled`, tanpa
        // menyentuh Redis (terbukti lolos tanpa server Redis).
        for provider in ["github", "google"] {
            let app = oauth_router(test_state(test_config("", "")));
            let req = Request::builder()
                .uri(format!("/api/auth/oauth/{provider}/start/?next_path=/foo"))
                .header("host", "api.test")
                .body(Body::empty())
                .unwrap();
            let resp = app.oneshot(req).await.unwrap();
            assert!(resp.status().is_redirection(), "{provider} must 302");
            assert_eq!(location_of(&resp), "http://web:3000/?error=oauth_disabled");
        }
    }

    #[tokio::test]
    async fn callback_unknown_provider_or_bad_query_redirects_oauth_error() {
        let app = oauth_router(test_state(test_config("id", "secret")));
        for uri in [
            "/api/auth/oauth/gitlab/callback/?code=x&state=y".to_string(),
            "/api/auth/oauth/github/callback/".to_string(),
            "/api/auth/oauth/github/callback/?code=&state=".to_string(),
        ] {
            let req = Request::builder()
                .uri(&uri)
                .header("host", "api.test")
                .body(Body::empty())
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert!(resp.status().is_redirection(), "{uri} must 302");
            assert_eq!(
                location_of(&resp),
                "http://web:3000/sign-in?error=oauth",
                "{uri}"
            );
        }
    }

    #[tokio::test]
    async fn callback_bogus_state_redirects_oauth_error() {
        // State acak tak pernah disimpan → GETDEL nil (atau Redis down →
        // dipetakan ke redirect yang sama) → 302 sign-in?error=oauth.
        let app = oauth_router(test_state(test_config("id", "secret")));
        let req = Request::builder()
            .uri("/api/auth/oauth/github/callback/?code=dummy&state=deadbeef00")
            .header("host", "api.test")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert!(resp.status().is_redirection());
        assert_eq!(location_of(&resp), "http://web:3000/sign-in?error=oauth");
    }

    /// Bukti live bila Redis tersedia (default suite tetap lolos tanpa Redis):
    /// start dengan kredensial dummy menulis state ke Redis lalu 302 ke
    /// provider; nilai state terbaca balik via GETDEL langsung.
    #[tokio::test]
    #[ignore]
    async fn live_start_stores_state_and_redirects_to_provider() {
        let cfg = test_config("dummy-id", "dummy-secret");
        let redis_client = redis::Client::open(cfg.redis_url.as_str()).expect("redis client open");
        let mut conn = redis_client
            .get_connection_manager()
            .await
            .expect("redis up for live test");
        let app = oauth_router(test_state(cfg));
        let req = Request::builder()
            .uri("/api/auth/oauth/github/start/?next_path=/foo")
            .header("host", "api.test")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert!(resp.status().is_redirection());
        let loc = location_of(&resp);
        assert!(
            loc.starts_with("https://github.com/login/oauth/authorize?"),
            "unexpected location: {loc}"
        );
        assert!(loc.contains("client_id=dummy-id"));
        let state = loc
            .split("state=")
            .nth(1)
            .expect("auth_url carries state")
            .split('&')
            .next()
            .unwrap()
            .to_string();
        assert_eq!(state.len(), 32, "state is uuid simple hex");
        let stored: Option<String> = redis::cmd("GETDEL")
            .arg(format!("auth:oauth:{state}"))
            .query_async(&mut conn)
            .await
            .expect("redis GETDEL");
        assert_eq!(stored.as_deref(), Some("github:/foo"));
    }
}
