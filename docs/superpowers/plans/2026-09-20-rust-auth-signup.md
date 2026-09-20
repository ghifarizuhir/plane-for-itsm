# Rust Auth Sign-up (Invite-only) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Endpoint `POST /api/auth/signup/` di api-rs (invite-only + auto-join) dan form sign-up web pindah dari native form POST ke Django menjadi JSON fetch, sehingga member terundang bisa membuat akun tanpa email confirmation.

**Architecture:** Modul baru `routes/auth_signup.rs` (handler + helper murni + unit test), didaftarkan di `auth_router` (`main.rs`) agar ikut rate-limit IP 5/menit. Membuat `users` + `profiles` (pola `instance_admin::sign_up`), auto-join semua invite pending → `workspace_members` + hapus invite + set `profiles.last_workspace_id`, lalu terbitkan cookie JWT persis jalur login. Frontend `password.tsx` memakai pola `handleSignIn` (fetch JSON + banner error).

**Tech Stack:** Rust (axum 0.7, sqlx, redis, zxcvbn 3), React/TypeScript (React Router app), i18n JSON, smoke bash + psql.

**Spec:** `docs/superpowers/specs/2026-09-20-rust-auth-signup-design.md`

**Penting:** Semua perintah `cargo` dijalankan dari `apps/api-rs`. Perintah `docker compose`/`pnpm` dari root repo.

---

## File map

- Modify: `apps/api-rs/crates/api/src/routes/auth.rs` — `set_cookies` → `pub(crate)`.
- Modify: `apps/api-rs/crates/api/src/routes/instance_admin.rs` — `user_agent_of` → `pub(crate)`.
- Modify: `apps/api-rs/crates/api/src/routes/invite.rs` — `default_ws_member_props` → `pub(crate)`.
- Create: `apps/api-rs/crates/api/src/routes/auth_signup.rs` — handler + helper + unit test.
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs` — `pub mod auth_signup;`.
- Modify: `apps/api-rs/crates/api/src/main.rs` — route `/api/auth/signup/` di `auth_router`.
- Modify: `apps/web/core/components/account/auth-forms/password.tsx` — JSON signup + banner error.
- Modify: `packages/i18n/src/locales/en/auth.json` — key error baru.
- Modify: `apps/api-rs/scripts/smoke.sh` — siklus signup + `DB_CONTAINER` + cleanup/proof.

---

### Task 1: Expose helper yang akan dipakai ulang

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/auth.rs:38`
- Modify: `apps/api-rs/crates/api/src/routes/instance_admin.rs:155`
- Modify: `apps/api-rs/crates/api/src/routes/invite.rs:671`

- [ ] **Step 1: Ubah visibilitas tiga helper**

Di `routes/auth.rs` ubah baris:

```rust
fn set_cookies(headers: &mut HeaderMap, at: &str, rt: &str, secure: bool) {
```

menjadi:

```rust
pub(crate) fn set_cookies(headers: &mut HeaderMap, at: &str, rt: &str, secure: bool) {
```

Di `routes/instance_admin.rs` ubah baris:

```rust
fn user_agent_of(headers: &HeaderMap) -> String {
```

menjadi:

```rust
pub(crate) fn user_agent_of(headers: &HeaderMap) -> String {
```

Di `routes/invite.rs` ubah baris:

```rust
fn default_ws_member_props() -> (Value, Value, Value) {
```

menjadi:

```rust
pub(crate) fn default_ws_member_props() -> (Value, Value, Value) {
```

- [ ] **Step 2: Verifikasi kompilasi**

Run: `cargo check -p api`
Expected: `Finished` di baris terakhir, tidak ada baris `error`.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/auth.rs apps/api-rs/crates/api/src/routes/instance_admin.rs apps/api-rs/crates/api/src/routes/invite.rs
git commit -m "refactor(api-rs): expose set_cookies, user_agent_of, default_ws_member_props for signup reuse"
```

---

### Task 2: Modul `auth_signup.rs` — helper murni + unit test (TDD)

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/auth_signup.rs`
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs`

- [ ] **Step 1: Tulis file modul dengan helper + test (gagal: handler belum ada, tapi test helper bisa hijau — bagian handler ditambahkan Task 3)**

Isi lengkap `apps/api-rs/crates/api/src/routes/auth_signup.rs`:

```rust
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
    }

    #[test]
    fn raw_required_semantics() {
        assert!(signup_fields_missing("", "whatever1!"));
        assert!(signup_fields_missing("a@b.co", ""));
        // Spasi dianggap terisi (nilai raw), sama seperti Python truthiness.
        assert!(!signup_fields_missing("   ", "whatever1!"));
    }

    #[test]
    fn error_body_shape() {
        let body = auth_error(SIGNUP_DISABLED, "SIGNUP_DISABLED");
        assert_eq!(body.0["error_code"], 5015);
        assert_eq!(body.0["error_message"], "SIGNUP_DISABLED");
    }
}
```

- [ ] **Step 2: Daftarkan modul**

Di `apps/api-rs/crates/api/src/routes/mod.rs`, setelah baris `pub mod auth_compat;` tambahkan:

```rust
pub mod auth_signup;
```

- [ ] **Step 3: Jalankan test**

Run: `cargo test -p api auth_signup`
Expected: `test result: ok. 3 passed; 0 failed;` di baris lib test.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/auth_signup.rs apps/api-rs/crates/api/src/routes/mod.rs
git commit -m "feat(api-rs): signup error helpers with unit tests"
```

---

### Task 3: Handler signup + wiring route

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/auth_signup.rs` (isi lengkap final)
- Modify: `apps/api-rs/crates/api/src/main.rs:1757-1775` (`auth_router`)

- [ ] **Step 1: Tulis handler lengkap — ganti seluruh isi `auth_signup.rs` dengan ini**

```rust
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

use crate::routes::auth::{email_valid, set_cookies};
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

/// `if not email or not password` Django pada nilai RAW (sebelum strip):
/// string berisi spasi dianggap terisi → lolos ke validasi email berikutnya.
pub fn signup_fields_missing(email_raw: &str, password_raw: &str) -> bool {
    email_raw.is_empty() || password_raw.is_empty()
}

fn family_key(family: &str) -> String {
    format!("auth:family:{family}")
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
    .await?;
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
        .await?;
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
    .await?;
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

    let mut tx = st.pool.begin().await?;

    // users — kolom/nilai persis `instance_admin::sign_up` (`:594-616`),
    // `is_email_verified=false` (default password signup Django).
    let (uid,): (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO users (id, email, username, password, first_name, last_name, display_name, \
         avatar, date_joined, token, user_timezone, last_location, created_location, \
         last_login_ip, last_logout_ip, last_login_medium, last_login_uagent, last_active, \
         last_login_time, token_updated_at, is_active, is_staff, is_superuser, is_managed, \
         is_password_expired, is_email_verified, is_password_autoset, is_bot, is_email_valid, \
         is_password_reset_required, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, '', '', $4, '', now(), $5, 'UTC', '', '', \
         $6, '', 'email', $7, now(), now(), now(), true, false, false, false, false, false, \
         false, false, true, false, now(), now()) RETURNING id",
    )
    .bind(&email)
    .bind(&username)
    .bind(&hash)
    .bind(&display_name)
    .bind(&token)
    .bind(&ip)
    .bind(&ua)
    .fetch_one(&mut *tx)
    .await?;

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
    .await?;

    // Auto-join: semua invite pending → WorkspaceMember (reactivate/insert),
    // row invite di-hard-delete; `last_workspace_id` = workspace pertama.
    let invites: Vec<(uuid::Uuid, uuid::Uuid, i16)> = sqlx::query_as(
        "SELECT id, workspace_id, role FROM workspace_member_invites \
         WHERE email = $1 AND deleted_at IS NULL AND responded_at IS NULL \
         ORDER BY created_at ASC",
    )
    .bind(&email)
    .fetch_all(&mut *tx)
    .await?;
    let mut first_workspace: Option<uuid::Uuid> = None;
    for (invite_id, workspace_id, role) in &invites {
        let existing: Option<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT id FROM workspace_members WHERE workspace_id = $1 AND member_id = $2",
        )
        .bind(workspace_id)
        .bind(uid)
        .fetch_optional(&mut *tx)
        .await?;
        if existing.is_some() {
            sqlx::query(
                "UPDATE workspace_members SET is_active = true, role = $1, updated_at = now() \
                 WHERE workspace_id = $2 AND member_id = $3",
            )
            .bind(role)
            .bind(workspace_id)
            .bind(uid)
            .execute(&mut *tx)
            .await?;
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
            .await?;
        }
        sqlx::query("DELETE FROM workspace_member_invites WHERE id = $1")
            .bind(invite_id)
            .execute(&mut *tx)
            .await?;
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
        .await?;
    }
    tx.commit().await?;

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
    }

    #[test]
    fn raw_required_semantics() {
        assert!(signup_fields_missing("", "whatever1!"));
        assert!(signup_fields_missing("a@b.co", ""));
        // Spasi dianggap terisi (nilai raw), sama seperti Python truthiness.
        assert!(!signup_fields_missing("   ", "whatever1!"));
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
```

- [ ] **Step 2: Wire route di `main.rs`**

Di `apps/api-rs/crates/api/src/main.rs`, dalam `let auth_router = Router::new()` (sekitar baris 1757), setelah baris:

```rust
        .route("/api/auth/login/", post(routes::auth::login))
```

tambahkan:

```rust
        .route("/api/auth/signup/", post(routes::auth_signup::signup))
```

- [ ] **Step 3: Compile + test**

Run: `cargo test -p api`
Expected: seluruh test suite hijau (`test result: ok`), termasuk `auth_signup` 4 test. Tidak ada baris `error[`.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/auth_signup.rs apps/api-rs/crates/api/src/main.rs
git commit -m "feat(api-rs): invite-only signup endpoint with auto-join and session cookies"
```

---

### Task 4: Frontend — JSON signup + banner error + i18n

**Files:**

- Modify: `packages/i18n/src/locales/en/auth.json`
- Modify: `apps/web/core/components/account/auth-forms/password.tsx`

- [ ] **Step 1: Muat skill `translate`, lalu tambah key i18n**

**WAJIB:** panggil skill `translate` sebelum mengedit file locale (aturan repo).

Di `packages/i18n/src/locales/en/auth.json`, di dalam `"sign_up"`, ubah blok `"errors"` menjadi:

```json
    "errors": {
      "password": {
        "strength": "Try setting-up a strong password to proceed"
      },
      "email_exists": "An account with this email already exists. Try signing in instead.",
      "email_invalid": "Enter a valid email address.",
      "signup_disabled": "Sign-up is invite-only. Ask your workspace admin to invite you first."
    }
```

Verifikasi JSON valid:

```bash
python3 -c "import json; d=json.load(open('packages/i18n/src/locales/en/auth.json')); print(sorted(d['auth']['sign_up']['errors'].keys()))"
```

Expected: `['email_exists', 'email_invalid', 'password', 'signup_disabled']`

- [ ] **Step 2: Ganti import `password.tsx`**

Di `apps/web/core/components/account/auth-forms/password.tsx` ubah baris 7 dari:

```tsx
import { useEffect, useMemo, useRef, useState } from "react";
```

menjadi:

```tsx
import { useMemo, useState } from "react";
```

Hapus baris import `AuthService`:

```tsx
// services
import { AuthService } from "@/services/auth.service";
```

Hapus juga baris konstanta:

```tsx
const authService = new AuthService();
```

- [ ] **Step 3: Bersihkan state/efek CSRF dan tambah state error signup**

Hapus baris:

```tsx
// ref
const formRef = useRef<HTMLFormElement>(null);
```

Ubah blok state dari:

```tsx
const [csrfPromise, setCsrfPromise] = useState<Promise<{ csrf_token: string }> | undefined>(undefined);
const [passwordFormData, setPasswordFormData] = useState<TPasswordFormValues>({ ...defaultValues, email });
```

menjadi:

```tsx
const [passwordFormData, setPasswordFormData] = useState<TPasswordFormValues>({ ...defaultValues, email });
```

Ubah state banner dari:

```tsx
const [isBannerMessage, setBannerMessage] = useState(false);
```

menjadi:

```tsx
const [isBannerMessage, setBannerMessage] = useState(false);
const [signUpErrorCode, setSignUpErrorCode] = useState<number | null>(null);
```

Hapus seluruh blok `useEffect` CSRF:

```tsx
useEffect(() => {
  // CSRF hanya dibutuhkan mode SIGN_UP (POST native ke Django).
  // SIGN_IN login JSON ke Rust (/api/auth/login/, cookie HttpOnly) tanpa CSRF.
  if (mode !== EAuthModes.SIGN_UP) return;
  if (csrfPromise === undefined) {
    const promise = authService.requestCSRFToken();
    setCsrfPromise(promise);
  }
}, [csrfPromise, mode]);
```

- [ ] **Step 4: Ganti `handleSignUp` dan hapus `handleCSRFToken`**

Hapus blok `handleCSRFToken`:

```tsx
const handleCSRFToken = async () => {
  if (!formRef || !formRef.current) return;
  const token = await csrfPromise;
  if (!token?.csrf_token) return;
  const csrfElement = formRef.current.querySelector("input[name=csrfmiddlewaretoken]");
  csrfElement?.setAttribute("value", token?.csrf_token);
};
```

Ganti `handleSignUp` menjadi:

```tsx
const handleSignUp = async () => {
  if (isSubmitting) return;
  setSignUpErrorCode(null);
  const isPasswordValid = getPasswordStrength(passwordFormData.password) === E_PASSWORD_STRENGTH.STRENGTH_VALID;
  if (!isPasswordValid) {
    setSignUpErrorCode(5021);
    return;
  }
  setIsSubmitting(true);
  try {
    const res = await fetch(`${API_BASE_URL}/api/auth/signup/`, {
      method: "POST",
      credentials: "include",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ email: passwordFormData.email, password: passwordFormData.password }),
    });
    if (res.ok) {
      window.location.assign(nextPath || "/");
      return;
    }
    const data = (await res.json().catch(() => ({}))) as { error_code?: number };
    setSignUpErrorCode(data?.error_code ?? 0);
  } catch {
    setSignUpErrorCode(0);
  } finally {
    setIsSubmitting(false);
  }
};
```

- [ ] **Step 5: Tambah pesan error hasil mapping**

Sebelum `return (`, setelah deklarasi `renderPasswordMatchError`, tambahkan:

```tsx
const signUpErrorMessage = useMemo(() => {
  switch (signUpErrorCode) {
    case 5021:
      return t("auth.sign_up.errors.password.strength");
    case 5030:
      return t("auth.sign_up.errors.email_exists");
    case 5015:
      return t("auth.sign_up.errors.signup_disabled");
    case 5045:
      return t("auth.sign_up.errors.email_invalid");
    default:
      return t("something_went_wrong_please_try_again");
  }
}, [signUpErrorCode, t]);
```

- [ ] **Step 6: Ganti banner SIGN_UP**

Ganti blok JSX banner:

```tsx
{
  isBannerMessage && mode === EAuthModes.SIGN_UP && (
    <div className="relative flex items-center gap-2 rounded-md border border-danger-strong/50 bg-danger-subtle p-2">
      <div className="relative flex h-4 w-4 shrink-0 items-center justify-center">
        <InfoOutline width={16} height={16} className="text-danger-primary" />
      </div>
      <div className="w-full text-13 font-medium text-danger-primary">{t("auth.sign_up.errors.password.strength")}</div>
      <button
        type="button"
        className="relative ml-auto flex h-6 w-6 cursor-pointer items-center justify-center rounded-xs text-accent-primary/80 transition-all hover:bg-danger-subtle-hover"
        onClick={() => setBannerMessage(false)}
      >
        <CloseOutline className="h-4 w-4 shrink-0 text-danger-primary" />
      </button>
    </div>
  );
}
```

menjadi:

```tsx
{
  signUpErrorCode !== null && mode === EAuthModes.SIGN_UP && (
    <div className="relative flex items-center gap-2 rounded-md border border-danger-strong/50 bg-danger-subtle p-2">
      <div className="relative flex h-4 w-4 shrink-0 items-center justify-center">
        <InfoOutline width={16} height={16} className="text-danger-primary" />
      </div>
      <div className="w-full text-13 font-medium text-danger-primary">{signUpErrorMessage}</div>
      <button
        type="button"
        className="relative ml-auto flex h-6 w-6 cursor-pointer items-center justify-center rounded-xs text-accent-primary/80 transition-all hover:bg-danger-subtle-hover"
        onClick={() => setSignUpErrorCode(null)}
      >
        <CloseOutline className="h-4 w-4 shrink-0 text-danger-primary" />
      </button>
    </div>
  );
}
```

- [ ] **Step 7: Bersihkan `<form>` dari jalur native**

Ganti pembuka `<form>`:

```tsx
      <form
        ref={formRef}
        className="space-y-4"
        method="POST"
        action={`${API_BASE_URL}/auth/${mode === EAuthModes.SIGN_IN ? "sign-in" : "sign-up"}/`}
        onSubmit={async (event) => {
```

menjadi:

```tsx
      <form
        className="space-y-4"
        onSubmit={async (event) => {
```

Hapus tiga hidden input:

```tsx
        <input type="hidden" name="csrfmiddlewaretoken" />
        <input type="hidden" value={passwordFormData.email} name="email" />
        {nextPath && <input type="hidden" value={nextPath} name="next_path" />}
```

- [ ] **Step 8: Typecheck, lint, build**

Run: `pnpm --filter=web exec tsc --noEmit -p tsconfig.json 2>&1 | head -n 30`
Expected: tidak ada output (exit 0).

Run: `pnpm exec oxlint apps/web/core/components/account/auth-forms/password.tsx 2>&1 | tail -n 5`
Expected: `Found 0 warnings and 0 errors.`

Run: `pnpm --filter=web build 2>&1 | tail -n 3`
Expected: `SPA Mode: Generated build/client/index.html` (exit 0).

- [ ] **Step 9: Commit**

```bash
git add packages/i18n/src/locales/en/auth.json apps/web/core/components/account/auth-forms/password.tsx
git commit -m "feat(web): signup posts JSON to rust api with invite-only error banner"
```

---

### Task 5: Smoke coverage signup

**Files:**

- Modify: `apps/api-rs/scripts/smoke.sh`

- [ ] **Step 1: Tambah `DB_CONTAINER` dan pakai di cleanup/proof**

Setelah baris `BASE="${BASE:-http://127.0.0.1:8000}"` tambahkan:

```bash
# Nama container DB dapat dioverride (compose project prefix berbeda-beda,
# mis. `plane-for-itsm-plane-db-1`).
DB_CONTAINER="${DB_CONTAINER:-plane-db}"
```

Ganti **semua pemakaian perintah** `docker exec plane-db` di script (17 occurrence: baris 333, 341, 342, 344, 357, 358, 366, 368, 371, 376, 489, 573, 574, 575, 576, 578, 582) menjadi `docker exec "$DB_CONTAINER"`. Jangan ubah contoh di komentar header baris 16.

Verifikasi tidak ada sisa:

```bash
grep -n "docker exec plane-db" apps/api-rs/scripts/smoke.sh
```

Expected: hanya baris 16 (komentar header).

- [ ] **Step 2: Tambah seksi signup setelah blok `== auth ==` (`fi`)**

```bash
echo "== signup (invite-only) =="
# XFF terpisah agar tidak memakan budget IP 5/mnt auth_router (login dkk).
SIGNUP_EMAIL="temp-signup-$SFX@example.com"
SIGNUP_NOINVITE="temp-signup-noinvite-$SFX@example.com"
SIGNUP_JAR=/tmp/smoke_signup_jar
SIGNUP_ARGS=(-s -m 10 -H "X-Forwarded-For: 203.0.113.9" -H 'Content-Type: application/json' -H "Origin: $FRONTEND")
signup_check() { # signup_check <label> <expected_status> <curl_args...>
  local label="$1" want="$2"; shift 2
  local code
  code=$(curl "${SIGNUP_ARGS[@]}" -o /tmp/smoke_body -w '%{http_code}' "$@")
  if [ "$code" = "$want" ]; then PASS=$((PASS+1)); echo "ok   $label -> $code";
  else FAIL=$((FAIL+1)); FAILED="$FAILED $label($code)"; echo "FAIL $label -> $code want $want: $(head -c 200 /tmp/smoke_body)"; fi
}
# 1) tanpa invite → 403 {5015}
signup_check signup-noinvite-403 403 -X POST -d "{\"email\":\"$SIGNUP_NOINVITE\",\"password\":\"Smoke-Signup-42!Zq\"}" "$BASE/api/auth/signup/"
grep -q '"error_code":5015' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   signup-noinvite-body -> 5015"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED signup-noinvite-body"; echo "FAIL signup-noinvite-body: $(head -c 200 /tmp/smoke_body)"; }
# 2) invite lewat API (di luar auth_router → bebas limit IP)
check signup-invite-create 200 -X POST -d "{\"emails\":[{\"email\":\"$SIGNUP_EMAIL\",\"role\":15}]}" "$BASE/api/workspaces/$WS/invitations/"
# 3) password lemah dengan invite → 400 {5021}
signup_check signup-weak-400 400 -X POST -d "{\"email\":\"$SIGNUP_EMAIL\",\"password\":\"password\"}" "$BASE/api/auth/signup/"
grep -q '"error_code":5021' /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   signup-weak-body -> 5021"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED signup-weak-body"; echo "FAIL signup-weak-body: $(head -c 200 /tmp/smoke_body)"; }
# 4) happy path → 200 + cookie sesi
rm -f "$SIGNUP_JAR"
signup_check signup-200 200 -c "$SIGNUP_JAR" -X POST -d "{\"email\":\"$SIGNUP_EMAIL\",\"password\":\"Smoke-Signup-42!Zq\"}" "$BASE/api/auth/signup/"
grep -q "$SIGNUP_EMAIL" /tmp/smoke_body && { PASS=$((PASS+1)); echo "ok   signup-body -> email"; } || { FAIL=$((FAIL+1)); FAILED="$FAILED signup-body"; echo "FAIL signup-body: $(head -c 200 /tmp/smoke_body)"; }
# 5) cookie hasil signup langsung valid + auto-join workspace
code=$(curl -s -m 10 -b "$SIGNUP_JAR" -H "Origin: $FRONTEND" -o /tmp/smoke_body -w '%{http_code}' "$BASE/api/users/me/workspaces/")
if [ "$code" = "200" ] && grep -q "$WS" /tmp/smoke_body; then PASS=$((PASS+1)); echo "ok   signup-autologin-join -> $WS";
else FAIL=$((FAIL+1)); FAILED="$FAILED signup-autologin-join($code)"; echo "FAIL signup-autologin-join -> $code: $(head -c 200 /tmp/smoke_body)"; fi
rm -f "$SIGNUP_JAR"
```

- [ ] **Step 3: Tambah cleanup + proof**

Di seksi `== temp-user cleanup ... ==` tambahkan baris (setelah baris temp-user yang ada):

```bash
docker exec "$DB_CONTAINER" psql -U plane -d plane -q -c "DELETE FROM workspace_members WHERE member_id IN (SELECT id FROM users WHERE email LIKE 'temp-signup-%'); DELETE FROM workspace_member_invites WHERE email LIKE 'temp-signup-%'; DELETE FROM profiles WHERE user_id IN (SELECT id FROM users WHERE email LIKE 'temp-signup-%'); DELETE FROM users WHERE email LIKE 'temp-signup-%';" 2>&1 | head -n 1
```

Di seksi `== leftover-proof ... ==` tambahkan setelah `z-tempusers`:

```bash
proof_zero z-signupusers "SELECT COUNT(*) FROM users WHERE email LIKE 'temp-signup-%'"
```

- [ ] **Step 4: Cek sintaks bash**

Run: `bash -n apps/api-rs/scripts/smoke.sh`
Expected: tidak ada output (exit 0).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/scripts/smoke.sh
git commit -m "test(api-rs): smoke signup invite-only cycle with db container override"
```

---

### Task 6: Build, deploy, verifikasi live

**Files:**

- None (verifikasi + rollout)

- [ ] **Step 1: Rebuild image api-rs**

Run (root repo): `docker compose -f docker-compose-local.yml build api`
Expected: build sukses (BuildKit selesai tanpa error; release build Rust bisa beberapa menit).

- [ ] **Step 2: Recreate service api/worker/beat-worker**

Run: `docker compose -f docker-compose-local.yml up -d api worker beat-worker`
Expected: container `plane-for-itsm-api-1`, `plane-for-itsm-worker-1`, `plane-for-itsm-beat-worker-1` status `Started`.

Run: `curl -s http://localhost:8000/health`
Expected: `{"status":"ok","service":"rust-api"}`

- [ ] **Step 3: Cek route baru aktif (tanpa Origin → 403 origin; bukti bukan 404)**

Run:

```bash
curl -s -o /dev/null -w "%{http_code}\n" -X POST http://localhost:8000/api/auth/signup/ -H "Content-Type: application/json" -d '{"email":"x@y.co","password":"z"}'
```

Expected: `403` (middleware Origin menolak mutasi tanpa Origin — bukan lagi `404 Page not found`).

Run:

```bash
NOINVITE="notinvited-$(date +%s)@example.com"
curl -s -X POST http://localhost:8000/api/auth/signup/ -H "Origin: http://192.168.1.11:3000" -H "Content-Type: application/json" -d "{\"email\":\"$NOINVITE\",\"password\":\"Smoke-Signup-42!Zq\"}"
```

Expected: `{"error_code":5015,"error_message":"SIGNUP_DISABLED"}` (invite-only bekerja).

- [ ] **Step 4: Build frontend + restart prod**

Run: `pnpm --filter=web build`
Expected: `SPA Mode: Generated build/client/index.html`

Run: `systemctl --user restart plane-web-prod.service && systemctl --user is-active plane-web-prod.service`
Expected: `active`

- [ ] **Step 5: Jalankan smoke penuh**

Ambil token lalu jalankan (root repo):

```bash
TOKEN=$(docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -t -A -c "SELECT token FROM api_tokens WHERE label = 'smoke' LIMIT 1")
TOKEN="$TOKEN" DB_CONTAINER=plane-for-itsm-plane-db-1 bash apps/api-rs/scripts/smoke.sh
```

Expected: baris `ok   signup-*` muncul, baris terakhir `PASS=<n> FAIL=0`, exit code 0.
(Blok `== auth ==` di-skip bila `SMOKE_EMAIL`/`SMOKE_PASSWORD` tidak di-set — itu wajar.)

- [ ] **Step 6: E2E manual browser (tunnel)**

1. Admin: Workspace Settings → Members → invite email member baru (atau pakai invite pending yang sudah ada: `andaranata45@gmail.com`).
2. Member: buka web → masukkan email → set password → **Create account** tanpa pindah ke host API.
3. Expected: masuk onboarding; workspace hasil auto-join ada di daftar; tanpa langkah email.
4. Negatif: signup dengan email tanpa invite → banner merah "Sign-up is invite-only...".
5. Cek DB: invite row untuk email itu terhapus, `workspace_members` memuat anggota baru, `profiles.last_workspace_id` ter-set.

- [ ] **Step 7: Catat rollback**

Bila perlu rollback: `git revert` commit Task 4/5/3/2/1 → rebuild image + build web (Step 1, 4). Akun yang sudah dibuat tetap valid (hash kompatibel Django).

---

## Self-review checklist (sudah dijalankan saat menulis plan)

- Spec coverage: gateway invite-only → Task 3 Step 1; auto-join + last_workspace_id → Task 3; cookie/session → Task 3; error codes/status → Task 2+3; frontend JSON + banner + i18n → Task 4; smoke → Task 5; rollout → Task 6.
- Placeholder scan: tidak ada TBD/TODO("implement later"); semua code block lengkap.
- Type consistency: `signup_error_status`, `signup_fields_missing`, `err`, `family_key` konsisten antar Task 2/3; key i18n `auth.sign_up.errors.*` sama antara Task 4 Step 1 dan Step 5; nama kode error konstanta sama.
