# Auth Asli Rust (Irisan 1 Paritas Penuh) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Login email+password dan OAuth (GitHub/Google) dilayani Rust dengan cookie JWT ganda; frontend ikut diubah; Django tidak lagi dibutuhkan untuk auth.

**Architecture:** `common::auth` (hash PBKDF2 format-Django, JWT HS256, refresh opaque di Redis) + `routes/auth.rs` (login/refresh/logout/OAuth/me) + upgrade middleware `AuthUser` (Bearer JWT → cookie → X-Api-Key). Frontend: interceptor 401→refresh→retry, form password jadi JSON fetch, URL OAuth ke Rust.

**Tech Stack:** Rust Axum 0.7, `jsonwebtoken` 9 (sudah ada), baru: `pbkdf2` + `sha2` + `base64` (verifikasi hash Django), `reqwest` (OAuth code exchange). Frontend: axios interceptor yang ada di `apps/web/core/services/api.service.ts`.

**Konvensi yang mengikat semua task:**

- Cookie: `plane_at` (access JWT, 15 mnt) + `plane_rt` (refresh opaque, 30 hari). Flags: `HttpOnly; Path=/; SameSite=Lax` + `Secure; __Host-` prefix hanya bila `COOKIE_SECURE=1`.
- Redis: `auth:refresh:{sha256hex(refresh)}` → `{user_id}:{family}` TTL 30 hari; `auth:oauth:{state}` → `{provider}:{next_path}` TTL 10 menit.
- JWT claims: `{sub: user_id, exp, iat, jti}`. Secret: `JWT_SECRET` env (wajib di prod; default dev `"dev-only-insecure"` + `tracing::warn!` bila default dipakai).
- Error auth: selalu `401 {"error": "..."}` generik; tidak membedakan email-tak-terdaftar vs password-salah.
- Iterasi Django live: `1000000` (lihat `users.password` aktual). Parser harus tolak format tak dikenal.

---

### Task 1: Dependensi + konfigurasi

**Files:**

- Modify: `apps/api-rs/Cargo.toml`
- Modify: `apps/api-rs/crates/api/Cargo.toml`
- Modify: `apps/api-rs/crates/common/Cargo.toml`
- Modify: `apps/api-rs/crates/common/src/config.rs`
- Test: `apps/api-rs/crates/common/tests/config_test.rs` (baru)

- [ ] **Step 1: Tambah dependensi workspace**

```toml
# apps/api-rs/Cargo.toml — tambah di [workspace.dependencies]
pbkdf2 = { version = "0.12" }
sha2 = "0.10"
base64 = "0.22"
reqwest = { version = "0.12", features = ["json", "rustls-tls-manual-roots"] }
```

```toml
# crates/api/Cargo.toml [dependencies] — tambah
pbkdf2 = { workspace = true }
sha2 = { workspace = true }
base64 = { workspace = true }
reqwest = { workspace = true }
```

```toml
# crates/common/Cargo.toml [dependencies] — tambah
pbkdf2 = { workspace = true }
sha2 = { workspace = true }
base64 = { workspace = true }
jsonwebtoken = "9"
```

- [ ] **Step 2: Perluas AppConfig**

```rust
// crates/common/src/config.rs — tambah field + baca env
pub struct AppConfig {
    pub database_url: String,
    pub redis_url: String,
    pub port: u16,
    pub jwt_secret: String,
    pub cookie_secure: bool,
    pub frontend_url: String,
    pub github_client_id: String,
    pub github_client_secret: String,
    pub google_client_id: String,
    pub google_client_secret: String,
}
// from_env tambahan:
jwt_secret: env::var("JWT_SECRET").unwrap_or_else(|_| {
    tracing::warn!("JWT_SECRET unset — using insecure dev default");
    "dev-only-insecure".into()
}),
cookie_secure: env::var("COOKIE_SECURE").map(|v| v == "1").unwrap_or(false),
frontend_url: env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:3000".into()),
github_client_id: env::var("GITHUB_CLIENT_ID").unwrap_or_default(),
github_client_secret: env::var("GITHUB_CLIENT_SECRET").unwrap_or_default(),
google_client_id: env::var("GOOGLE_CLIENT_ID").unwrap_or_default(),
google_client_secret: env::var("GOOGLE_CLIENT_SECRET").unwrap_or_default(),
```

Catatan: `config.rs` saat ini tidak import tracing — tambah `use tracing;` atau ganti warn dengan `eprintln!`. Pilih `eprintln!` (common tidak depend ke tracing; jangan tambah dep baru hanya untuk ini).

- [ ] **Step 3: Tulis test config**

```rust
// crates/common/tests/config_test.rs
#[test]
fn cookie_secure_defaults_off() {
    std::env::remove_var("COOKIE_SECURE");
    let cfg = common::config::AppConfig::from_env();
    assert!(!cfg.cookie_secure);
    assert_eq!(cfg.frontend_url, "http://localhost:3000");
}
```

- [ ] **Step 4: Run test, harapkan FAIL (field belum ada)**

Run: `cargo test -p common --test config_test`
Expected: FAIL — compile error `no field cookie_secure`.

- [ ] **Step 5: Implementasi Step 2, run ulang**

Run: `cargo test -p common --test config_test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/Cargo.toml apps/api-rs/crates/api/Cargo.toml apps/api-rs/crates/common/Cargo.toml apps/api-rs/crates/common/src/config.rs apps/api-rs/crates/common/tests/config_test.rs
git commit -m "feat(rs-auth): deps + config JWT/OAuth/cookie"
```

---

### Task 2: Verifikasi password format-Django

**Files:**

- Create: `apps/api-rs/crates/common/src/auth.rs`
- Modify: `apps/api-rs/crates/common/src/lib.rs`
- Test: `apps/api-rs/crates/common/tests/django_hash_test.rs` (baru)

Vektor uji (dihasilkan via `hashlib.pbkdf2_hmac('sha256', b'plan-vector-ok', b'smokesalt12345678', 1000000)`):
`pbkdf2_sha256$1000000$smokesalt12345678$NyV386oOw4JpTzGNeUTSCrrCWBd/LOQxHJY3lHfakhI=`

- [ ] **Step 1: Tulis failing test**

```rust
// crates/common/tests/django_hash_test.rs
const VECTOR: &str = "pbkdf2_sha256$1000000$smokesalt12345678$NyV386oOw4JpTzGNeUTSCrrCWBd/LOQxHJY3lHfakhI=";

#[test]
fn accepts_correct_password() {
    assert!(common::auth::verify_django_password("plan-vector-ok", VECTOR));
}

#[test]
fn rejects_wrong_password() {
    assert!(!common::auth::verify_django_password("salah", VECTOR));
}

#[test]
fn rejects_unknown_format() {
    assert!(!common::auth::verify_django_password("x", "bcrypt$abc"));
    assert!(!common::auth::verify_django_password("x", ""));
}

#[test]
fn make_then_verify_roundtrip() {
    let h = common::auth::make_django_password("rahasia-baru");
    assert!(h.starts_with("pbkdf2_sha256$1000000$"));
    assert!(common::auth::verify_django_password("rahasia-baru", &h));
}
```

- [ ] **Step 2: Run, harapkan FAIL**

Run: `cargo test -p common --test django_hash_test`
Expected: FAIL — `no module auth`.

- [ ] **Step 3: Implementasi minimal**

```rust
// crates/common/src/auth.rs
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;

const ITERATIONS: u32 = 1_000_000;

/// Verifikasi hash format Django `pbkdf2_sha256$iter$salt$b64`.
/// False untuk format tak dikenal — tidak pernah panic pada input asing.
pub fn verify_django_password(password: &str, encoded: &str) -> bool {
    let mut parts = encoded.split('$');
    match (parts.next(), parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("pbkdf2_sha256"), Some(iter), Some(salt), Some(hash), None) => {
            let iter: u32 = match iter.parse() {
                Ok(n) => n,
                Err(_) => return false,
            };
            let expected = match base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                hash,
            ) {
                Ok(b) => b,
                Err(_) => return false,
            };
            let mut out = vec![0u8; expected.len()];
            pbkdf2_hmac::<Sha256>(password.as_bytes(), salt.as_bytes(), iter, &mut out);
            out.len() == expected.len()
                && out.iter().zip(expected.iter()).fold(0, |a, (x, y)| a | (x ^ y)) == 0
        }
        _ => false,
    }
}

/// Hash password baru dalam format Django (agar kompatibel dua arah).
pub fn make_django_password(password: &str) -> String {
    let salt: String = uuid::Uuid::new_v4().simple().to_string()[..12].into();
    let mut out = vec![0u8; 32];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt.as_bytes(), ITERATIONS, &mut out);
    format!(
        "pbkdf2_sha256${ITERATIONS}${salt}${}",
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, out)
    )
}
```

```rust
// crates/common/src/lib.rs — tambah
pub mod auth;
```

- [ ] **Step 4: Run, harapkan PASS**

Run: `cargo test -p common --test django_hash_test`
Expected: PASS (4 test).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/common/src/auth.rs apps/api-rs/crates/common/src/lib.rs apps/api-rs/crates/common/tests/django_hash_test.rs
git commit -m "feat(rs-auth): verifikasi + buat password format-Django"
```

---

### Task 3: JWT access + builder cookie

**Files:**

- Modify: `apps/api-rs/crates/common/src/auth.rs`
- Test: `apps/api-rs/crates/common/tests/jwt_cookie_test.rs` (baru)

- [ ] **Step 1: Tulis failing test**

```rust
// crates/common/tests/jwt_cookie_test.rs
#[test]
fn access_roundtrip() {
    let uid = uuid::Uuid::new_v4();
    let tok = common::auth::encode_access(&uid, "s3cr3t", 900);
    let got = common::auth::decode_access(&tok, "s3cr3t").expect("valid");
    assert_eq!(got, uid);
}

#[test]
fn wrong_secret_rejected() {
    let uid = uuid::Uuid::new_v4();
    let tok = common::auth::encode_access(&uid, "s3cr3t", 900);
    assert!(common::auth::decode_access(&tok, "lain").is_err());
}

#[test]
fn cookie_headers_shape() {
    let dev = common::auth::cookie_headers("plane_at", "abc", 900, false);
    assert!(dev.contains("plane_at=abc"));
    assert!(dev.contains("HttpOnly"));
    assert!(dev.contains("SameSite=Lax"));
    assert!(!dev.contains("Secure"));
    let prod = common::auth::cookie_headers("__Host-plane_at", "abc", 900, true);
    assert!(prod.contains("Secure"));
}

#[test]
fn clear_cookie_expires_immediately() {
    let h = common::auth::clear_cookie_header("plane_at", false);
    assert!(h.contains("Max-Age=0"));
}
```

- [ ] **Step 2: Run, harapkan FAIL**

Run: `cargo test -p common --test jwt_cookie_test`
Expected: FAIL — fungsi belum ada.

- [ ] **Step 3: Implementasi**

```rust
// tambah di crates/common/src/auth.rs
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct AccessClaims {
    sub: String,
    exp: usize,
    iat: usize,
    jti: String,
}

pub fn encode_access(user_id: &uuid::Uuid, secret: &str, ttl_secs: i64) -> String {
    let now = chrono::Utc::now().timestamp();
    let claims = AccessClaims {
        sub: user_id.to_string(),
        exp: (now + ttl_secs) as usize,
        iat: now as usize,
        jti: uuid::Uuid::new_v4().to_string(),
    };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
        .expect("jwt encode")
}

pub fn decode_access(token: &str, secret: &str) -> Result<uuid::Uuid, String> {
    let mut v = Validation::default();
    v.validate_exp = true;
    decode::<AccessClaims>(token, &DecodingKey::from_secret(secret.as_bytes()), &v)
        .map_err(|e| e.to_string())?
        .claims
        .sub
        .parse()
        .map_err(|e| format!("bad sub: {e}"))
}

pub fn cookie_headers(name: &str, value: &str, max_age: i64, secure: bool) -> String {
    let mut h = format!("{name}={value}; HttpOnly; Path=/; Max-Age={max_age}; SameSite=Lax");
    if secure {
        h.push_str("; Secure");
    }
    h
}

pub fn clear_cookie_header(name: &str, secure: bool) -> String {
    let mut h = format!("{name}=; HttpOnly; Path=/; Max-Age=0; SameSite=Lax");
    if secure {
        h.push_str("; Secure");
    }
    h
}
```

`common/Cargo.toml` butuh `jsonwebtoken = "9"` (sudah di Step Task 1).

- [ ] **Step 4: Run, harapkan PASS**

Run: `cargo test -p common --test jwt_cookie_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/common/src/auth.rs apps/api-rs/crates/common/tests/jwt_cookie_test.rs
git commit -m "feat(rs-auth): JWT access + builder cookie"
```

---

### Task 4: Refresh opaque di Redis + Origin check

**Files:**

- Modify: `apps/api-rs/crates/common/src/auth.rs`
- Create: `apps/api-rs/crates/api/src/middleware/origin.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs` (registrasi layer)
- Test: `apps/api-rs/crates/api/tests/origin_test.rs` (baru)

Refresh: opaque `rt_` + 32 hex; simpan sha256hex di Redis 30 hari.
Fungsi Redis live — diuji via smoke (Task 9); di sini hanya konstanta key + Origin middleware yang unit-testable.

- [ ] **Step 1: Tulis failing test Origin**

```rust
// crates/api/tests/origin_test.rs
use api::middleware::origin::origin_allowed;
use axum::http::{HeaderMap, Method};

fn headers(origin: Option<&str>, referer: Option<&str>) -> HeaderMap {
    let mut h = HeaderMap::new();
    if let Some(o) = origin {
        h.insert("origin", o.parse().unwrap());
    }
    if let Some(r) = referer {
        h.insert("referer", r.parse().unwrap());
    }
    h
}

#[test]
fn get_always_allowed() {
    assert!(origin_allowed(&Method::GET, &headers(None, None), "https://app.example.com"));
}

#[test]
fn post_matching_origin_allowed() {
    let h = headers(Some("https://app.example.com"), None);
    assert!(origin_allowed(&Method::POST, &h, "https://app.example.com"));
}

#[test]
fn post_foreign_origin_rejected() {
    let h = headers(Some("https://evil.example"), None);
    assert!(!origin_allowed(&Method::POST, &h, "https://app.example.com"));
}

#[test]
fn post_referer_fallback_allowed() {
    let h = headers(None, Some("https://app.example.com/sign-in"));
    assert!(origin_allowed(&Method::POST, &h, "https://app.example.com"));
}

#[test]
fn post_no_origin_no_referer_rejected() {
    assert!(!origin_allowed(&Method::POST, &headers(None, None), "https://app.example.com"));
}
```

- [ ] **Step 2: Run, harapkan FAIL**

Run: `cargo test -p api --test origin_test`
Expected: FAIL — modul belum ada.

- [ ] **Step 3: Implementasi middleware + helper Redis**

```rust
// crates/api/src/middleware/origin.rs
use axum::{
    extract::Request,
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::json;

/// Pure check (unit-testable). GET/HEAD/OPTIONS selalu lolos; mutasi wajib
/// Origin cocok, fallback Referer prefix-cocok.
pub fn origin_allowed(method: &Method, headers: &axum::http::HeaderMap, frontend: &str) -> bool {
    if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return true;
    }
    if let Some(o) = headers.get("origin").and_then(|v| v.to_str().ok()) {
        return o == frontend;
    }
    if let Some(r) = headers.get("referer").and_then(|v| v.to_str().ok()) {
        return r.starts_with(frontend);
    }
    false
}

pub async fn origin_middleware(
    axum::extract::State(frontend): axum::extract::State<String>,
    req: Request,
    next: Next,
) -> Response {
    if !origin_allowed(req.method(), req.headers(), &frontend) {
        return (StatusCode::FORBIDDEN, axum::Json(json!({"error": "bad origin"}))).into_response();
    }
    next.run(req).await
}
```

```rust
// tambah di crates/common/src/auth.rs
pub fn new_refresh() -> (String, String) {
    // (refresh_mentah, sha256hex_untuk_redis)
    let raw = format!("rt_{}", uuid::Uuid::new_v4().simple());
    (sha256hex(&raw), raw)
}

pub fn sha256hex(s: &str) -> String {
    use sha2::Digest;
    hex_of(&sha2::Sha256::digest(s.as_bytes()))
}

fn hex_of(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn refresh_key(hash: &str) -> String {
    format!("auth:refresh:{hash}")
}
pub fn oauth_key(state: &str) -> String {
    format!("auth:oauth:{state}")
}
```

Catatan: `new_refresh` mengembalikan `(hash, raw)` — urutan ini dipakai Task 5
(`let (hash, raw) = new_refresh();` simpan hash, kirim raw ke cookie).

- [ ] **Step 4: Daftarkan `mod origin` + layer di main.rs**

```rust
// crates/api/src/main.rs — tambah di deklarasi modul (lihat `mod middleware;`)
// struktur aktual: middleware adalah file/modul; tambah route-layer:
// .route_layer(axum_middleware::from_fn_with_state(cfg.frontend_url.clone(), origin_middleware))
// Letakkan SETELAH route_layer rate-limit yang ada (lihat main.rs:419).
```

Perintah layer persis (tempel setelah blok rate-limit):

```rust
let app = app.route_layer(axum_middleware::from_fn_with_state(
    cfg.frontend_url.clone(),
    crate::middleware::origin::origin_middleware,
));
```

`middleware` adalah modul (`crates/api/src/middleware/mod.rs` berisi
`pub mod auth; pub mod rate_limit;`) — tambah baris `pub mod origin;` di sana.

- [ ] **Step 5: Run test, harapkan PASS**

Run: `cargo test -p api --test origin_test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/common/src/auth.rs apps/api-rs/crates/api/src/middleware/ apps/api-rs/crates/api/src/main.rs apps/api-rs/crates/api/tests/origin_test.rs
git commit -m "feat(rs-auth): refresh-redis helper + origin check"
```

---

### Task 5: Login / refresh / logout + upgrade AuthUser

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/auth.rs`
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs`
- Modify: `apps/api-rs/crates/api/src/middleware/auth.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Modify: `apps/api-rs/crates/api/src/routes/misc.rs` (helper `user_id` → deprecated,Hz pertahankan hingga Task 6 selesai; tandai `#[allow(dead_code)]` bila tak dipakai)
- Test: live via `smoke.sh` (Task 9) — handler butuh pool+redis, pola repo belum punya DB-backed test (lihat `lib.rs:test_app` stub-only).

Rute baru:

- `POST /api/auth/login/` `{email, password}` → 200 `{id, email}` + 2 Set-Cookie. Rate-limit 5/mnt/IP (Task 10 pasang; siapkan handler tanpa limit dulu).
- `POST /api/auth/refresh/` (baca cookie `plane_rt`/`__Host-plane_rt`) → rotasi: hapus hash lama, terbit pasangan baru → 200 + 2 Set-Cookie. Reuse (hash tak dikenal) → 401 (catat: deteksi pencurian = revoke keluarga — implementasi: simpan `family` di value Redis, hapus semua key keluarga via SCAN — bila SCAN terlalu berat, revoke = hapus semua refresh user tersebut dengan secondary index `auth:family:{family}` berupa SET member hash. Implementasi secondary index ini.)
- `POST /api/auth/logout/` → hapus hash refresh + clear kedua cookie → 200.
- Upgrade `AuthUser`: `pub struct AuthUser(pub uuid::Uuid)` — urutan: `Authorization: Bearer <jwt>` (verifikasi `decode_access` dengan `JWT_SECRET` dari state — extractor butuh pool? TIDAK: secret + dari config. `FromRequestParts` dengan `S = AppState`: ambil `State<AppState>` di dalam extractor via `axum::extract::State::<AppState>::from_request_parts`) → cookie `plane_at`/`__Host-plane_at` → `X-Api-Key` (query `api_tokens` seperti helper lama, tambah cek `is_active` + `expired_at IS NULL OR > now()`).
- Semua pemakai `auth.0` sebagai STRING TOKEN hari ini (`misc.rs`, `user.rs`, dll.) harus diganti memakai `auth.0` sebagai UUID user langsung — hapus pemanggilan `user_id(&st, &auth)`. Daftar file terdampak: `grep -rn "user_id(&st" crates/api/src/routes/`.

- [ ] **Step 1: Ubah `AuthUser` + sesuaikan semua pemakai** (kompilasi sebagai verifikasi; tanpa test baru — perilaku like dijaga smoke Task 9)

Perubahan inti `middleware/auth.rs`:

```rust
use axum::{extract::{FromRequestParts, State}, http::{request::Parts, StatusCode}, response::{IntoResponse, Response}};
use crate::state::AppState;

#[derive(Debug, Clone)]
pub struct AuthUser(pub uuid::Uuid);

#[async_trait::async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = Response;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let unauthorized = || (StatusCode::UNAUTHORIZED, "missing or invalid auth").into_response();
        // 1. Bearer JWT
        if let Some(tok) = parts.headers.get("Authorization").and_then(|v| v.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer ")) {
            if let Ok(uid) = common::auth::decode_access(tok.trim(), &state.config.jwt_secret) {
                return Ok(AuthUser(uid));
            }
        }
        // 2. Cookie access
        if let Some(uid) = cookie_uid(parts, state) {
            return Ok(AuthUser(uid));
        }
        // 3. X-Api-Key (DB lookup + aktif + belum expired)
        if let Some(key) = parts.headers.get("X-Api-Key").and_then(|v| v.to_str().ok()) {
            let row: Option<(uuid::Uuid,)> = sqlx::query_as(
                "SELECT user_id FROM api_tokens WHERE token = $1 AND is_active = true AND deleted_at IS NULL AND (expired_at IS NULL OR expired_at > now())",
            ).bind(key.trim()).fetch_optional(&state.pool).await.map_err(|_| unauthorized())?;
            if let Some((uid,)) = row {
                return Ok(AuthUser(uid));
            }
        }
        Err(unauthorized())
    }
}

fn cookie_uid(parts: &Parts, state: &AppState) -> Option<uuid::Uuid> {
    let cookies = parts.headers.get("cookie")?.to_str().ok()?;
    for pair in cookies.split(';') {
        let (k, v) = pair.trim().split_once('=')?;
        if k == "plane_at" || k == "__Host-plane_at" {
            if let Ok(uid) = common::auth::decode_access(v.trim(), &state.config.jwt_secret) {
                return Some(uid);
            }
        }
    }
    None
}
```

Konsekuensi: `AppState` butuh field `config: AppConfig` — tambah di `state.rs` + konstruksi di `main.rs` (`config: cfg.clone()`, `AppConfig` sudah `Clone`). Semua handler yang memakai `_auth: AuthUser` tetap kompilasi (tipe berubah tapi nama sama); yang memakai `auth.0` sebagai `&String` (bind token) WAJIB diubah — daftar dari grep di atas, ganti `user_id(&st, &auth)` menjadi `auth.0` langsung.

**AppState untuk test (wajib, kalau tidak stub test tidak kompilasi):**
`FromRequestParts<AppState>` menuntut state router bertipe `AppState`, sedangkan
stub `lib.rs::test_app()` tidak punya state. Perbaikan dalam task ini:

1. `state.rs`: tambah `pub config: common::config::AppConfig`, dan ubah
   `pub redis: redis::aio::ConnectionManager` → `pub redis: redis::Client`
   (Client lazy tanpa koneksi; tidak ada route api yang memakai `st.redis` —
   grep membuktikan; worker tidak tersentuh karena punya konstruksi sendiri).
2. `main.rs`: konstruksi menjadi
   `AppState { pool, redis: redis::Client::open(&cfg.redis_url).expect("redis client open failed"), config: cfg.clone() }`.
3. `lib.rs::test_app()`: bangun state uji dan pasang ke router:

```rust
pub async fn test_app() -> Router {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgres://plane:plane@127.0.0.1:5432/plane_test")
        .expect("lazy pool");
    let state = crate::state::AppState {
        pool,
        redis: redis::Client::open("redis://127.0.0.1:6379").expect("redis client"),
        config: common::config::AppConfig::from_env(),
    };
    Router::new()
        .route("/health", get(routes::health::health))
        .route("/api/workspaces/", get(stub_workspaces_list).post(stub_workspaces_list))
        .with_state(state)
}
```

`connect_lazy` + `Client::open` tidak membuka koneksi — test 401 tidak pernah
menyentuh DB/Redis sehingga tetap murni unit. `lib.rs` butuh `use axum::{routing::get, Router};` (sudah ada) — tidak ada perubahan `auth_test.rs` (tetap 401).

`connect_lazy` + `Client::open` tidak membuka koneksi — test 401 tidak pernah
menyentuh DB/Redis sehingga tetap murni unit. Tidak ada perubahan `auth_test.rs`
(stub handler tetap terima `AuthUser`, hanya tipenya berubah; test cuma cek 401).

- [ ] **Step 2: Tulis `routes/auth.rs` (login/refresh/logout)**

```rust
// crates/api/src/routes/auth.rs
use axum::{extract::State, http::{header, StatusCode}, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};
use common::auth as authn;

#[derive(Deserialize)]
pub struct LoginBody { pub email: String, pub password: String }

fn cookie_pair(secure: bool) -> (&'static str, &'static str) {
    if secure { ("__Host-plane_at", "__Host-plane_rt") } else { ("plane_at", "plane_rt") }
}

fn set_cookies(headers: &mut axum::http::HeaderMap, at: &str, rt: &str, secure: bool) {
    let (at_name, rt_name) = cookie_pair(secure);
    headers.append(header::SET_COOKIE, authn::cookie_headers(at_name, at, 900, secure).parse().unwrap());
    headers.append(header::SET_COOKIE, authn::cookie_headers(rt_name, rt, 30 * 24 * 3600, secure).parse().unwrap());
}

/// POST /api/auth/login/ — email+password lawan hash Django.
pub async fn login(
    State(st): State<AppState>,
    Json(body): Json<LoginBody>,
) -> Result<(StatusCode, axum::http::HeaderMap, Json<Value>), common::errors::AppError> {
    let email = body.email.to_lowercase().trim().to_string();
    let row: Option<(uuid::Uuid, String, String)> =
        sqlx::query_as("SELECT id, email, password FROM users WHERE email = $1 AND deleted_at IS NULL")
            .bind(&email).fetch_optional(&st.pool).await?;
    let Some((uid, db_email, hash)) = row else {
        return Ok((StatusCode::UNAUTHORIZED, Default::default(), Json(json!({"error": "invalid credentials"}))));
    };
    if !authn::verify_django_password(&body.password, &hash) {
        return Ok((StatusCode::UNAUTHORIZED, Default::default(), Json(json!({"error": "invalid credentials"}))));
    }
    let access = authn::encode_access(&uid, &st.config.jwt_secret, 900);
    let (hash_rt, raw_rt) = authn::new_refresh();
    let family = uuid::Uuid::new_v4().to_string();
    let mut conn = (*st.redis_client()).await.map_err(|e| anyhow::anyhow!(e))?;
    redis::cmd("SET").arg(authn::refresh_key(&hash_rt)).arg(format!("{uid}:{family}"))
        .arg("EX").arg(30 * 24 * 3600).query_async::<()>(&mut conn).await
        .map_err(|e| anyhow::anyhow!(e))?;
    redis::cmd("SADD").arg(format!("auth:family:{family}")).arg(&hash_rt)
        .query_async::<()>(&mut conn).await.map_err(|e| anyhow::anyhow!(e))?;
    redis::cmd("EXPIRE").arg(format!("auth:family:{family}")).arg(30 * 24 * 3600)
        .query_async::<()>(&mut conn).await.map_err(|e| anyhow::anyhow!(e))?;
    let mut headers = axum::http::HeaderMap::new();
    set_cookies(&mut headers, &access, &raw_rt, st.config.cookie_secure);
    Ok((StatusCode::OK, headers, Json(json!({"id": uid, "email": db_email}))))
}
```

CATATAN `st.redis_client()`: karena `AppState.redis` kini bertipe `redis::Client`
(Task ini), tambah helper di `state.rs`:
`impl AppState { pub async fn redis_client(&self) -> redis::RedisResult<redis::aio::ConnectionManager> { self.redis.get_connection_manager().await } }`.
`refresh`/`logout` mengikuti pola yang sama (baca cookie rt → GET hash →
cocok → DEL lama + SADD baru; tak dikenal → 401; logout → DEL + clear cookie).
Tulis keduanya dengan pola identik — tidak diulang di sini agar plan ringkas,
tapi kode wajib ada sebelum Step 3.

- [ ] **Step 3: Registrasi rute di main.rs + `pub mod auth;` di routes/mod.rs**

```rust
.route("/api/auth/login/", post(routes::auth::login))
.route("/api/auth/refresh/", post(routes::auth::refresh))
.route("/api/auth/logout/", post(routes::auth::logout))
```

- [ ] **Step 4: Verifikasi kompilasi + test lama hijau**

Run: `cargo check -p api` (harus 0 error), lalu `cargo test -p api --test auth_test` → PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/middleware/auth.rs apps/api-rs/crates/api/src/routes/auth.rs apps/api-rs/crates/api/src/routes/mod.rs apps/api-rs/crates/api/src/routes/misc.rs apps/api-rs/crates/api/src/routes/user.rs apps/api-rs/crates/api/src/routes/workspace.rs apps/api-rs/crates/api/src/routes/cycle.rs apps/api-rs/crates/api/src/state.rs apps/api-rs/crates/api/src/lib.rs apps/api-rs/crates/api/src/main.rs
git commit -m "feat(rs-auth): login/refresh/logout + AuthUser identitas UUID"
```

(grep dulu file-route apa saja yang memakai `auth.0` sebagai string dan ikutkan.)

---

### Task 6: OAuth GitHub + Google

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/auth.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Modify: `apps/api-rs/Cargo.toml` (reqwest sudah dari Task 1)

Desain: trait kecil agar callback unit-testable tanpa provider asli.

```rust
// di routes/auth.rs
#[async_trait::async_trait]
pub trait OAuthProvider {
    fn auth_url(&self, state: &str) -> String;
    async fn exchange(&self, code: &str) -> Result<String, String>; // -> verified email
}

pub struct GithubProvider { client_id: String, client_secret: String, http: reqwest::Client }
pub struct GoogleProvider { client_id: String, client_secret: String, http: reqwest::Client }

#[async_trait::async_trait]
impl OAuthProvider for GithubProvider {
    fn auth_url(&self, state: &str) -> String {
        format!("https://github.com/login/oauth/authorize?client_id={}&scope=user:email&state={}", self.client_id, state)
    }
    async fn exchange(&self, code: &str) -> Result<String, String> {
        // POST login/oauth/access_token (Accept: application/json) → access_token
        // GET api.github.com/user/emails (Bearer) → email primary+verified pertama
        // Err(String) bila tak ada email terverifikasi
    }
}

#[async_trait::async_trait]
impl OAuthProvider for GoogleProvider {
    fn auth_url(&self, state: &str) -> String {
        format!("https://accounts.google.com/o/oauth2/v2/auth?client_id={}&response_type=code&scope=openid%20email&state={}", self.client_id, state)
    }
    async fn exchange(&self, code: &str) -> Result<String, String> {
        // POST oauth2.googleapis.com/token → id_token/access_token
        // GET openidconnect.googleapis.com/v1/userinfo (Bearer) → email_verified + email
    }
}
```

Handler (sama untuk kedua provider, pilih via `:provider` = `github|google`):

- `GET /api/auth/oauth/:provider/start/?next_path=` → state=uuid hex → Redis
  `auth:oauth:{state}` = `{provider}:{next_path atau /}` EX 600 → 302 `auth_url(state)`.
  Bila client_id/secret kosong → 302 ke `{frontend}/?error=oauth_disabled`.
- `GET /api/auth/oauth/:provider/callback/?code=&state=` → ambil+hapus state
  Redis (tak ada → redirect `{frontend}/sign-in?error=oauth`) → `exchange(code)`
  → `find_or_create_user(email)`:

```sql
SELECT id FROM users WHERE email = $1 AND deleted_at IS NULL
-- tak ada: INSERT INTO users (id, email, username, password, first_name, last_name, display_name, avatar, date_joined, token, user_timezone, last_location, created_location, last_login_ip, last_logout_ip, last_login_medium, last_login_uagent, is_active, is_staff, is_superuser, is_managed, is_password_expired, is_email_verified, is_password_autoset, is_bot, is_email_valid, is_password_reset_required, created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, '!', '', '', $3, '', now(), '', 'UTC', '', '', '', '', 'oauth', '', true, false, false, false, false, true, false, false, true, false, now(), now()) RETURNING id
```

(`username` = uuid hex 32 char — pola `adapter/base.py:354`; `display_name` =
local-part email — pola `User.save`; password `'!'` = unusable ala Django;
`is_email_verified=true` karena email dari provider terverifikasi.)
→ set cookie seperti login → 302 ke `next_path`.

- [ ] **Step 1: Implementasi provider + handler + registrasi rute**
- [ ] **Step 2: `cargo check -p api` 0 error + `cargo test -p api` hijau**
- [ ] **Step 3: Verifikasi manual start** (butuh client_id dummy — cukup cek redirect):
      `curl -s -o /dev/null -w '%{http_code} %{redirect_url}\n' 127.0.0.1:8000/api/auth/oauth/github/start/` → 302 ke github.com (set GITHUB_CLIENT_ID dummy dulu).
- [ ] **Step 4: Commit** `feat(rs-auth): OAuth GitHub + Google`

---

### Task 7: (Opsional) GitLab + Gitea

Pola identik Task 6 (provider baru + 2 URL + field email berbeda).
Hanya kerjakan bila instance memakai keduanya; kalau tidak, SKIP dan catat di
spec sebagai follow-up. Commit terpisah bila dikerjakan.

---

### Task 8: Frontend — interceptor, form password, URL OAuth

**Files:**

- Modify: `apps/web/core/services/api.service.ts`
- Modify: `apps/web/core/components/account/auth-forms/password.tsx`
- Modify: `apps/web/core/hooks/oauth/core.tsx` (+ `extended.tsx` bila ada URL OAuth di sana)
- Modify: `packages/services/src/auth/auth.service.ts` (bila dipakai web — cek; samakan perubahan)

- [ ] **Step 1: Interceptor 401→refresh→retry** di `api.service.ts`:

```ts
private setupInterceptors() {
  this.axiosInstance.interceptors.response.use(
    (response) => response,
    async (error) => {
      const original = error.config as any;
      if (error.response?.status === 401 && !original._retried && !original.url?.includes("/api/auth/")) {
        original._retried = true;
        try {
          await this.axiosInstance.post("/api/auth/refresh/", {});
          return this.axiosInstance.request(original);
        } catch {
          /* jatuh ke redirect */
        }
      }
      if (error.response && error.response.status === 401) {
        const currentPath = window.location.pathname;
        window.location.replace(`/${currentPath ? `?next_path=${currentPath}` : ``}`);
      }
      return Promise.reject(error);
    }
  );
}
```

Catatan: `API_BASE_URL` + path `/api/auth/refresh/` — samakan prefix aktual
(`API_URL` vs `API_BASE_URL`; lihat `packages/constants/src/endpoints.ts:7-9`).

- [ ] **Step 2: Form password jadi JSON fetch** (`password.tsx`):
      hapus `csrfPromise`/`handleCSRFToken`/hidden `csrfmiddlewaretoken`;
      `onSubmit` → `fetch(${API_BASE_URL}/api/auth/login/, {method POST, credentials include, JSON {email, password}})` → OK → `window.location.assign(nextPath || "/")`; 401 → tampilkan error kredensial (pakai pola banner yang ada).
      Endpoint sign-up (`/api/auth/signup/`) BELUM ada di backend (non-goal irisan 1 —
      Django sign-up tetap untuk pendaftaran baru; catat TODO di kode mengarah ke spec).
- [ ] **Step 3: URL OAuth** (`core.tsx` + file oauth lain yang memakai
      `${API_BASE_URL}/auth/github/` dkk.): ganti ke
      `${API_BASE_URL}/api/auth/oauth/github/start/` (+ google; gitlab/gitea hanya
      bila Task 7 dikerjakan).
- [ ] **Step 4: Verifikasi**: `pnpm check:lint` + `pnpm check:types` untuk paket
      yang diubah; catat hasil di pesan commit bila ada warning pre-existing.
- [ ] **Step 5: Commit** `feat(web-auth): login lawan Rust, refresh-retry, OAuth URL`

---

### Task 9: Smoke auth + checklist E2E manual

**Files:**

- Modify: `apps/api-rs/scripts/smoke.sh`
- Create: `docs/superpowers/specs/2026-09-05-e2e-checklist.md` (atau tambah ke plan ini? — file terpisah agar bisa dicetak/dicentang)

- [ ] **Step 1: Tambah ke smoke.sh** (setelah writes, sebelum cleanup):

```bash
JAR=/tmp/smoke_jar
check login-200 200 -c "$JAR" -X POST -d '{"email":"smoke@example.com","password":"..."}' "$BASE/api/auth/login/"
check me-200 200 -b "$JAR" "$BASE/api/users/me/"
check refresh-200 200 -c "$JAR" -b "$JAR" -X POST "$BASE/api/auth/refresh/"
check logout-200 200 -b "$JAR" -X POST "$BASE/api/auth/logout/"
check post-logout-401 401 -b "$JAR" "$BASE/api/users/me/"
check login-bad-401 401 -X POST -d '{"email":"smoke@example.com","password":"salah"}' "$BASE/api/auth/login/"
check oauth-start-302 302 "$BASE/api/auth/oauth/github/start/"
```

`check` memakai `-w '%{http_code}'` sehingga 302 tertangkap. Untuk login perlu
user smoke riil: buat sekali via SQL (`users` INSERT minimal per Task 6 +
`make_django_password`), atau pakai user dev yang ada. Dokumentasikan di header
skrip. Kredensial JANGAN di-commit — baca dari env `SMOKE_EMAIL/SMOKE_PASSWORD`.

- [ ] **Step 2: Tulis checklist E2E** (click-path: buka `/sign-in` → login form →
      masuk workspace → reload (persist) → buat issue → logout → buka rute privat
      (redirect sign-in) → OAuth klik-sampai-redirect). Tiap baris: langkah,
      ekspektasi, kolom hasil kosong.
- [ ] **Step 3: Jalankan smoke.sh penuh** → 23 + 7 cek hijau.
- [ ] **Step 4: Commit** `test(rs-auth): smoke siklus auth + checklist E2E`

---

### Task 10: Rate-limit login 5/mnt per IP

`RateLimiter` yang ada adalah bucket global proses (bukan per-IP) — tidak cocok
untuk login. Tambah limiter per-IP yang memakai ulang `Bucket::allow`:

**Files:**

- Modify: `apps/api-rs/crates/api/src/middleware/rate_limit.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs`
- Test: `apps/api-rs/crates/api/tests/ip_limit_test.rs` (baru)

- [ ] **Step 1: Tulis failing test**

```rust
// crates/api/tests/ip_limit_test.rs
use api::middleware::rate_limit::IpRateLimiter;
use std::net::IpAddr;
use std::time::Duration;

#[test]
fn quota_then_reject_per_ip() {
    let lim = IpRateLimiter::new(2, Duration::from_secs(60));
    let ip: IpAddr = "10.0.0.1".parse().unwrap();
    assert!(lim.allow_ip(ip));
    assert!(lim.allow_ip(ip));
    assert!(!lim.allow_ip(ip));
    let other: IpAddr = "10.0.0.2".parse().unwrap();
    assert!(lim.allow_ip(other));
}
```

- [ ] **Step 2: Run, harapkan FAIL** (`IpRateLimiter` belum ada).

- [ ] **Step 3: Implementasi**

```rust
// tambah di middleware/rate_limit.rs
use std::{collections::HashMap, net::IpAddr};

#[derive(Debug, Clone)]
pub struct IpRateLimiter {
    buckets: Arc<Mutex<HashMap<IpAddr, Bucket>>>,
    quota: u64,
    per: Duration,
}

impl IpRateLimiter {
    pub fn new(quota: u64, per: Duration) -> Self {
        Self { buckets: Arc::new(Mutex::new(HashMap::new())), quota, per }
    }
    pub fn allow_ip(&self, ip: IpAddr) -> bool {
        let mut map = self.buckets.lock().unwrap();
        if map.len() > 10_000 {
            map.clear();
        }
        map.entry(ip).or_insert_with(|| Bucket::new(self.quota, self.per)).allow(Instant::now())
    }
}

/// Ekstrak IP: X-Forwarded-For pertama → ConnectInfo → loopback.
pub fn client_ip(req: &Request, fallback: IpAddr) -> IpAddr {
    if let Some(xff) = req.headers().get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = xff.split(',').next() {
            if let Ok(ip) = first.trim().parse() {
                return ip;
            }
        }
    }
    req.extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|c| c.0.ip())
        .unwrap_or(fallback)
}

pub async fn ip_rate_limit_middleware(
    State(lim): State<IpRateLimiter>,
    req: Request,
    next: Next,
) -> Response {
    let ip = client_ip(&req, IpAddr::from([127, 0, 0, 1]));
    if lim.allow_ip(ip) {
        next.run(req).await
    } else {
        (StatusCode::TOO_MANY_REQUESTS, axum::Json(json!({"error": "rate limit exceeded"}))).into_response()
    }
}
```

Cek import `rate_limit.rs`: sudah ada `Request, State, StatusCode, Next,
Response` — tambah `use serde_json::json;` bila belum ada.

Pasang HANYA di rute auth (jangan global!) via sub-router di `main.rs`:

```rust
let auth_router = Router::new()
    .route("/api/auth/login/", post(routes::auth::login))
    .route("/api/auth/oauth/:provider/callback/", get(routes::auth::oauth_callback))
    .route_layer(axum_middleware::from_fn_with_state(
        IpRateLimiter::new(5, std::time::Duration::from_secs(60)),
        ip_rate_limit_middleware,
    ));
// gabung: Router::new().merge(auth_router).merge(app_utama)
```

Agar `ConnectInfo` terisi, ubah serve di `main.rs` menjadi:

```rust
axum::serve(
    listener,
    app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
).await.unwrap();
```

(cek bentuk serve aktual di main.rs — saat ini `axum::serve(listener, app)`.)

- [ ] **Step 4: Run test, harapkan PASS** + `cargo check -p api` bersih.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/middleware/rate_limit.rs apps/api-rs/crates/api/src/main.rs apps/api-rs/crates/api/tests/ip_limit_test.rs
git commit -m "feat(rs-auth): rate-limit login 5/mnt per IP"
```

---

## Gate akhir irisan 1

- `cargo test --workspace` 0 failed (dengan `DATABASE_URL` ke `plane_test`).
- `smoke.sh` 30 cek hijau (23 lama + 7 auth).
- Rebuild `api`, E2E checklist manual dicentang semua.
- Baru lanjut irisan 2 (temuan E2E) — Django tetap jalan selama irisan 1.
